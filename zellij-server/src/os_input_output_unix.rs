use crate::os_input_output::{command_exists, AsyncReader, PaneEnv, TerminalRunChangedBeforeWrite};
use crate::panes::PaneId;

use nix::{
    fcntl::{fcntl, FcntlArg, OFlag},
    pty::{openpty, OpenptyResult, Winsize},
    sys::{
        signal::{kill, killpg, Signal},
        termios,
    },
    unistd,
};
use tokio::io::unix::AsyncFd;

use libc::{self, ioctl, TIOCGWINSZ, TIOCSWINSZ};
use signal_hook::consts::*;

use std::{
    collections::BTreeMap,
    fs::File,
    io,
    os::fd::{AsFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd},
    os::unix::{
        io::{AsRawFd, RawFd},
        process::CommandExt,
    },
    process::{Child, Command},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use zellij_utils::{errors::prelude::*, input::command::RunCommand};

pub use async_trait::async_trait;

/// Send `signal` to `pid` and keep the syscall result.
///
/// `ESRCH` means the process is already gone, which is the outcome the
/// caller wanted. Every other failure is returned.
fn signal_pid(pid: u32, signal: Signal) -> Result<()> {
    match kill(unistd::Pid::from_raw(pid as i32), Some(signal)) {
        Ok(()) => Ok(()),
        Err(nix::errno::Errno::ESRCH) => Ok(()),
        Err(errno) => Err(anyhow!("failed to send {signal} to pid {pid}: {errno}")),
    }
}

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
        let borrowed_fd = unsafe { BorrowedFd::borrow_raw(fd) };
        let flags = fcntl(borrowed_fd, FcntlArg::F_GETFL)
            .map_err(|e| io::Error::from_raw_os_error(e as i32))?;
        let mut oflags = OFlag::from_bits_truncate(flags);
        oflags.insert(OFlag::O_NONBLOCK);
        fcntl(borrowed_fd, FcntlArg::F_SETFL(oflags))
            .map_err(|e| io::Error::from_raw_os_error(e as i32))?;

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

fn read_available(fd: RawFd, max: usize) -> Result<Vec<u8>, io::Error> {
    let mut buf: Vec<u8> = Vec::with_capacity(max);
    let ret = loop {
        let ret = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.capacity()) };
        if ret < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        break ret;
    };
    // `set_len` past capacity is undefined. A correct `read` never reports
    // more bytes than it was asked for; refuse that result instead.
    accept_read(&mut buf, 0, ret)?;
    while !buf.is_empty() && buf.len() < buf.capacity() {
        let filled = buf.len();
        let more = unsafe {
            libc::read(
                fd,
                buf.as_mut_ptr().add(filled) as *mut libc::c_void,
                buf.capacity() - filled,
            )
        };
        if more < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        if more == 0 {
            break;
        }
        if accept_read(&mut buf, filled, more).is_err() {
            break;
        }
    }
    buf.shrink_to_fit();
    Ok(buf)
}

fn accept_read(buf: &mut Vec<u8>, filled: usize, got: isize) -> Result<(), io::Error> {
    let got = got as usize;
    let Some(end) = filled.checked_add(got) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "read returned more bytes than requested",
        ));
    };
    if end > buf.capacity() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "read returned more bytes than requested",
        ));
    }
    unsafe { buf.set_len(end) };
    Ok(())
}

#[async_trait]
impl AsyncReader for RawFdAsyncReader {
    async fn read_chunk(&mut self, max: usize) -> Result<Vec<u8>, io::Error> {
        let async_fd = self.get_async_fd()?;
        loop {
            let mut guard = async_fd.readable().await?;
            match guard.try_io(|inner| read_available(inner.get_ref().as_raw_fd(), max)) {
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

    let mut signals = match signal_hook::iterator::Signals::new(&[SIGINT, SIGTERM]) {
        Ok(signals) => signals,
        Err(error) => {
            // Without a signal iterator we cannot forward shutdown signals.
            // Fall back to a plain wait instead of failing the pane's exit
            // watcher, which would panic the PTY thread.
            log::warn!(
                "failed to install signal handler for child process pid {}: {}",
                id,
                error
            );
            return child
                .wait()
                .map(|status| status.code())
                .with_context(err_context);
        },
    };

    // returns the exit status, if any
    let mut should_exit = false;
    let mut attempts = 3;
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
            if let Err(err) = child.kill() {
                log::warn!("failed to kill child {}: {err}", child.id());
            }
            // This function owns the wait, so reap the killed child before
            // reporting it. Otherwise the pane leaves a zombie behind.
            let _ = child.wait();
            break 'handle_exit Ok(None);
        }
    }
}

fn handle_openpty(
    open_pty_res: OpenptyResult,
    cmd: RunCommand,
    quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
    terminal_id: u32,
    pane_env: &PaneEnv,
) -> Result<(RawFd, RawFd)> {
    let err_context = |cmd: &RunCommand| {
        format!(
            "failed to open PTY for command '{}'",
            cmd.command.to_string_lossy().to_string()
        )
    };

    if !command_exists(&cmd, pane_env) {
        return Err(ZellijError::CommandNotFound {
            terminal_id,
            command: cmd.command.to_string_lossy().to_string(),
        })
        .with_context(|| err_context(&cmd));
    }

    // primary side of pty and child fd
    let pid_primary = open_pty_res.master.into_raw_fd();
    let pid_secondary = open_pty_res.slave.into_raw_fd();

    let spawn_result = unsafe {
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
        for (name, value) in pane_env {
            match value {
                Some(value) => command.env(name, value),
                None => command.env_remove(name),
            };
        }
        command
            .args(&cmd.args)
            .env("ZELLIJ_PANE_ID", &format!("{}", terminal_id))
            .pre_exec(move || -> io::Result<()> {
                if libc::login_tty(pid_secondary) != 0 {
                    return Err(io::Error::last_os_error());
                }
                close_fds::close_open_fds(3, &[]);
                Ok(())
            })
            .spawn()
    };
    let child = match spawn_result {
        Ok(child) => child,
        Err(e) => {
            let _ = unistd::close(pid_primary);
            let _ = unistd::close(pid_secondary);
            return Err(e).with_context(|| err_context(&cmd));
        },
    };

    let child_id = child.id();
    thread::spawn(move || {
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
    winsize: Option<Winsize>,
    quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
    terminal_id: u32,
    pane_env: &PaneEnv,
) -> Result<(RawFd, RawFd)> {
    let err_context = || "failed to spawn child terminal".to_string();

    // Seed winsize before exec so the child never observes 0×0.
    match openpty(winsize.as_ref(), &orig_termios) {
        Ok(open_pty_res) => handle_openpty(open_pty_res, cmd, quit_cb, terminal_id, pane_env),
        Err(e) => match failover_cmd {
            Some(failover_cmd) => handle_terminal(
                failover_cmd,
                None,
                orig_termios,
                winsize,
                quit_cb,
                terminal_id,
                pane_env,
            )
            .with_context(err_context),
            None => Err::<(i32, i32), _>(e)
                .context("failed to start pty")
                .with_context(err_context)
                .to_log(),
        },
    }
}

/// One open PTY endpoint and the run it was admitted for.
#[derive(Clone)]
struct TerminalEndpoint {
    generation: Option<u64>,
    fd: Arc<OwnedFd>,
}

/// The Unix PTY backend. Manages native PTY file descriptors and signals.
#[derive(Clone)]
pub(crate) struct UnixPtyBackend {
    orig_termios: Arc<Mutex<Option<termios::Termios>>>,
    /// Each terminal owns a duplicated descriptor held by the registry;
    /// a write, resize or drain issued after the original endpoint
    /// closed still targets the same open file description instead of
    /// whatever descriptor number the kernel recycled. `generation`
    /// names the process run that endpoint belongs to. `None` means the
    /// descriptor is installed and not yet bound, so a stamped write
    /// is refused until [`Self::bind_terminal_generation`].
    terminal_id_to_raw_fd: Arc<Mutex<BTreeMap<u32, Option<TerminalEndpoint>>>>,
    /// Last nonzero winsize applied to each terminal id. Reused when opening a
    /// replacement PTY for a rerun so the child never starts at 0×0 even when
    /// the pane's cached geometry is unchanged.
    last_terminal_sizes: Arc<Mutex<BTreeMap<u32, (u16, u16, Option<u16>, Option<u16>)>>>,
    next_terminal_id_counter: Arc<AtomicU32>,
}

/// Try to write as many bytes from `buf` as possible to `fd` without blocking.
///
/// Loops on successful short writes and EINTR to drain as much as the kernel
/// will accept, but issues at most `MAX_WRITE_SYSCALLS_PER_CALL` write(2)
/// calls (EINTR retries included) so a consumer that takes a few bytes per
/// call cannot turn one call into hundreds of syscalls. On EAGAIN (fd buffer
/// full) or when the syscall budget is spent, stops and returns how many
/// bytes were written so far (which may be 0). The caller is expected to
/// re-queue any unwritten remainder.
fn try_write_to_fd(
    fd: RawFd,
    buf: &[u8],
) -> std::result::Result<usize, (usize, nix::errno::Errno)> {
    write_bounded(buf, MAX_WRITE_SYSCALLS_PER_CALL, |chunk| {
        unistd::write(unsafe { BorrowedFd::borrow_raw(fd) }, chunk)
    })
}

/// Upper bound on write(2) calls made by one `try_write_to_fd` call. Together
/// with the pty writer's per-turn call limit this bounds the syscalls a
/// terminal can spend in one turn (calls per turn x this value).
const MAX_WRITE_SYSCALLS_PER_CALL: usize = 4;

fn write_bounded(
    buf: &[u8],
    max_syscalls: usize,
    mut write: impl FnMut(&[u8]) -> nix::Result<usize>,
) -> std::result::Result<usize, (usize, nix::errno::Errno)> {
    let mut written = 0;
    let mut syscalls = 0;
    while written < buf.len() && syscalls < max_syscalls {
        syscalls += 1;
        match write(&buf[written..]) {
            Ok(0) => break, // fd returned 0 on non-empty buf; treat like EAGAIN
            Ok(n) => written += n,
            Err(nix::errno::Errno::EINTR) => continue,
            Err(nix::errno::Errno::EAGAIN) => break,
            Err(e) => return Err((written, e)),
        }
    }
    Ok(written)
}

impl UnixPtyBackend {
    pub fn new() -> Result<Self, io::Error> {
        let current_termios = termios::tcgetattr(io::stdin()).ok();
        if current_termios.is_none() {
            log::warn!("Starting a server without a controlling terminal, using the default termios configuration.");
        }
        Ok(Self {
            orig_termios: Arc::new(Mutex::new(current_termios)),
            terminal_id_to_raw_fd: Arc::new(Mutex::new(BTreeMap::new())),
            last_terminal_sizes: Arc::new(Mutex::new(BTreeMap::new())),
            next_terminal_id_counter: Arc::new(AtomicU32::new(0)),
        })
    }

    fn initial_winsize_for(&self, terminal_id: u32) -> Winsize {
        if let Ok(sizes) = self.last_terminal_sizes.lock() {
            if let Some((cols, rows, width_in_pixels, height_in_pixels)) = sizes.get(&terminal_id) {
                if *cols > 0 && *rows > 0 {
                    return Winsize {
                        ws_col: *cols,
                        ws_row: *rows,
                        ws_xpixel: width_in_pixels.unwrap_or(0),
                        ws_ypixel: height_in_pixels.unwrap_or(0),
                    };
                }
            }
        }
        Winsize {
            ws_col: 80,
            ws_row: 24,
            ws_xpixel: 0,
            ws_ypixel: 0,
        }
    }

    pub fn spawn_terminal(
        &self,
        cmd: RunCommand,
        failover_cmd: Option<RunCommand>,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        terminal_id: u32,
        pane_env: &PaneEnv,
    ) -> Result<(Box<dyn AsyncReader>, RawFd)> {
        let orig_termios = self
            .orig_termios
            .lock()
            .to_anyhow()
            .context("failed to lock orig_termios")?;
        let winsize = self.initial_winsize_for(terminal_id);
        if let Ok(mut sizes) = self.last_terminal_sizes.lock() {
            sizes.insert(
                terminal_id,
                (
                    winsize.ws_col,
                    winsize.ws_row,
                    Some(winsize.ws_xpixel).filter(|v| *v > 0),
                    Some(winsize.ws_ypixel).filter(|v| *v > 0),
                ),
            );
        }
        let (pid_primary, child_fd) = handle_terminal(
            cmd,
            failover_cmd,
            orig_termios.clone(),
            Some(winsize),
            quit_cb,
            terminal_id,
            pane_env,
        )?;
        // On failure past this point, closing the primary side hangs up the
        // child's session, so the child exits through its normal watcher
        // instead of running without a pane.
        self.retain_primary_handle(terminal_id, pid_primary, |fd| {
            // try_clone_to_owned duplicates with close-on-exec, so the copy
            // is not inherited by helper processes spawned without a PTY.
            unsafe { BorrowedFd::borrow_raw(fd) }.try_clone_to_owned()
        })?;
        let async_reader = match RawFdAsyncReader::new(pid_primary) {
            Ok(async_reader) => Box::new(async_reader) as Box<dyn AsyncReader>,
            Err(e) => {
                if let Ok(mut registry) = self.terminal_id_to_raw_fd.lock() {
                    registry.remove(&terminal_id);
                }
                let _ = unistd::close(pid_primary);
                return Err(anyhow::anyhow!("failed to create async reader: {}", e));
            },
        };
        Ok((async_reader, child_fd))
    }

    /// Duplicate the primary side with `duplicate` and register the owned copy
    /// for `terminal_id`. When duplication fails, drop a reserved placeholder
    /// but keep a previously bound endpoint: a failed rerun must leave the old
    /// run addressable. The new primary side is closed before reporting the
    /// error, so no stale descriptor stays open.
    fn retain_primary_handle(
        &self,
        terminal_id: u32,
        pid_primary: RawFd,
        duplicate: impl FnOnce(RawFd) -> io::Result<OwnedFd>,
    ) -> Result<()> {
        match duplicate(pid_primary) {
            Ok(pty_handle) => {
                // Generation stays unbound until the PTY thread records the
                // run. A stamped write during that gap cannot land on this
                // descriptor, and the insert replaces any previous endpoint
                // before the new one is visible.
                self.terminal_id_to_raw_fd.lock().to_anyhow()?.insert(
                    terminal_id,
                    Some(TerminalEndpoint {
                        generation: None,
                        fd: Arc::new(pty_handle),
                    }),
                );
                Ok(())
            },
            Err(e) => {
                if let Ok(mut registry) = self.terminal_id_to_raw_fd.lock() {
                    // `Some(Some(_))` is the previous run's bound endpoint on
                    // a rerun; only remove a placeholder or a missing entry.
                    if !matches!(registry.get(&terminal_id), Some(Some(_))) {
                        registry.remove(&terminal_id);
                    }
                }
                let _ = unistd::close(pid_primary);
                Err(anyhow::anyhow!(
                    "failed to retain an owned PTY handle: {}",
                    e
                ))
            },
        }
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
            Some(Some(endpoint)) => {
                if cols > 0 && rows > 0 {
                    // Always apply to the current endpoint. A rerun opens a new
                    // PTY whose size must be written even when it equals the
                    // previous run's cached dimensions.
                    set_terminal_size_using_fd(
                        endpoint.fd.as_raw_fd(),
                        cols,
                        rows,
                        width_in_pixels,
                        height_in_pixels,
                    );
                    if let Ok(mut sizes) = self.last_terminal_sizes.lock() {
                        sizes.insert(
                            terminal_id,
                            (cols, rows, width_in_pixels, height_in_pixels),
                        );
                    }
                }
            },
            _ => {
                // Remember the intended size even when the endpoint is not yet
                // open so the next spawn seeds openpty with a nonzero winsize.
                if cols > 0 && rows > 0 {
                    if let Ok(mut sizes) = self.last_terminal_sizes.lock() {
                        sizes.insert(
                            terminal_id,
                            (cols, rows, width_in_pixels, height_in_pixels),
                        );
                    }
                }
                Err::<(), _>(anyhow!("failed to find terminal fd for id {terminal_id}"))
                    .with_context(err_context)
                    .non_fatal();
            },
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn write_to_tty_stdin(&self, terminal_id: u32, buf: &[u8]) -> Result<usize> {
        self.write_to_tty_stdin_for_run(terminal_id, None, buf)
    }

    /// Copy the descriptor only when `generation` is the run bound to it.
    /// The check and the clone happen under one lock, so a replacement
    /// cannot swap the endpoint between the check and the handle this
    /// write will use. The write itself runs after the lock is released.
    fn terminal_fd_for_run(
        &self,
        terminal_id: u32,
        generation: Option<u64>,
    ) -> Result<Arc<OwnedFd>> {
        let err_context = || format!("failed to write to stdin of TTY ID {}", terminal_id);

        let registry = self
            .terminal_id_to_raw_fd
            .lock()
            .to_anyhow()
            .with_context(err_context)?;
        match registry.get(&terminal_id) {
            Some(Some(endpoint)) => {
                if let Some(expected) = generation {
                    if endpoint.generation != Some(expected) {
                        return Err(anyhow::Error::new(TerminalRunChangedBeforeWrite));
                    }
                }
                Ok(endpoint.fd.clone())
            },
            _ => Err(anyhow!("could not find raw file descriptor")).with_context(err_context),
        }
    }

    pub fn write_to_tty_stdin_for_run(
        &self,
        terminal_id: u32,
        generation: Option<u64>,
        buf: &[u8],
    ) -> Result<usize> {
        let err_context = || format!("failed to write to stdin of TTY ID {}", terminal_id);
        let fd = self.terminal_fd_for_run(terminal_id, generation)?;
        match try_write_to_fd(fd.as_raw_fd(), buf) {
            Ok(written) => Ok(written),
            Err((_written, error)) => Err(anyhow::Error::new(error)).with_context(err_context),
        }
    }

    pub fn write_to_tty_stdin_for_run_with_progress(
        &self,
        terminal_id: u32,
        generation: Option<u64>,
        buf: &[u8],
    ) -> (usize, Result<()>) {
        let err_context = || format!("failed to write to stdin of TTY ID {}", terminal_id);
        let fd = match self.terminal_fd_for_run(terminal_id, generation) {
            Ok(fd) => fd,
            Err(error) => return (0, Err(error)),
        };
        match try_write_to_fd(fd.as_raw_fd(), buf) {
            Ok(written) => (written, Ok(())),
            Err((written, error)) => (
                written,
                Err(anyhow::Error::new(error)).with_context(err_context),
            ),
        }
    }

    pub fn bind_terminal_generation(&self, terminal_id: u32, generation: u64) {
        let Ok(mut registry) = self.terminal_id_to_raw_fd.lock() else {
            return;
        };
        if let Some(Some(endpoint)) = registry.get_mut(&terminal_id) {
            endpoint.generation = Some(generation);
        }
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
            Some(Some(endpoint)) => termios::tcdrain(endpoint.fd.as_fd()).with_context(err_context),
            _ => Err(anyhow!("could not find raw file descriptor")).with_context(err_context),
        }
    }

    pub fn tcgetpgrp(&self, terminal_id: u32) -> Option<i32> {
        match self.terminal_id_to_raw_fd.lock().ok()?.get(&terminal_id) {
            Some(Some(endpoint)) => unistd::tcgetpgrp(endpoint.fd.as_fd())
                .ok()
                .map(|pgid| pgid.as_raw()),
            _ => None,
        }
    }

    pub fn kill(&self, pid: u32) -> Result<()> {
        signal_pid(pid, Signal::SIGHUP)
    }

    pub fn force_kill(&self, pid: u32) -> Result<()> {
        signal_pid(pid, Signal::SIGKILL)
    }

    pub fn send_sigint(&self, pid: u32) -> Result<()> {
        signal_pid(pid, Signal::SIGINT)
    }

    pub fn send_sigint_to_terminal(&self, terminal_id: u32, shell_pid: u32) -> Result<()> {
        self.signal_terminal_foreground(terminal_id, shell_pid, Signal::SIGINT)
    }

    pub fn force_kill_terminal(&self, terminal_id: u32, shell_pid: u32) -> Result<()> {
        self.signal_terminal_foreground(terminal_id, shell_pid, Signal::SIGKILL)
    }

    /// Signal the terminal's foreground process group so applications such
    /// as `vim` receive it, falling back to the shell pid when the group
    /// cannot be determined or signalled.
    fn signal_terminal_foreground(
        &self,
        terminal_id: u32,
        shell_pid: u32,
        signal: Signal,
    ) -> Result<()> {
        if let Some(pgid) = self.tcgetpgrp(terminal_id).filter(|pgid| *pgid > 0) {
            match killpg(unistd::Pid::from_raw(pgid), Some(signal)) {
                Ok(()) | Err(nix::errno::Errno::ESRCH) => return Ok(()),
                Err(error) => log::debug!(
                    "failed to send {signal} to foreground process group {pgid} of terminal {terminal_id}: {error}; falling back to shell pid {shell_pid}"
                ),
            }
        }
        signal_pid(shell_pid, signal)
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
        if let Ok(mut sizes) = self.last_terminal_sizes.lock() {
            sizes.remove(&terminal_id);
        }
    }

    pub fn next_terminal_id(&self) -> Option<u32> {
        Some(
            self.next_terminal_id_counter
                .fetch_add(1, Ordering::Relaxed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nix::fcntl::{fcntl, FcntlArg, OFlag};
    use nix::sys::termios;
    use std::io::{Read, Write};

    #[test]
    fn a_retained_pty_handle_never_targets_a_recycled_descriptor() {
        // The registry keeps a duplicated OwnedFd per terminal. When the
        // original descriptor number comes to name an unrelated file, a
        // write through the registry must still reach the terminal's own
        // pipe. dup2 replaces the number atomically, so no other thread can
        // take it in between.
        let backend = UnixPtyBackend::new().expect("backend");
        let (terminal_read, terminal_write) = nonblocking_pipe();
        let (unrelated_read, unrelated_write) = nonblocking_pipe();

        let original = unsafe { OwnedFd::from_raw_fd(terminal_write) };
        let retained = original.try_clone().expect("dup original");
        backend.terminal_id_to_raw_fd.lock().unwrap().insert(
            7,
            Some(TerminalEndpoint {
                generation: None,
                fd: Arc::new(retained),
            }),
        );

        // The original number now names the unrelated pipe, as if the
        // pane's endpoint closed and the number was recycled.
        assert_eq!(
            unsafe { libc::dup2(unrelated_write, original.as_raw_fd()) },
            original.as_raw_fd()
        );

        let written = backend
            .write_to_tty_stdin(7, b"pane bytes")
            .expect("write must reach the retained endpoint");
        assert_eq!(written, b"pane bytes".len());

        let mut buf = [0u8; 16];
        let read = unsafe { libc::read(terminal_read, buf.as_mut_ptr() as *mut _, buf.len()) };
        assert_eq!(read, written as isize);
        assert_eq!(&buf[..written], b"pane bytes");
        let leaked = unsafe { libc::read(unrelated_read, buf.as_mut_ptr() as *mut _, buf.len()) };
        assert_eq!(leaked, -1, "nothing may reach the recycled descriptor");

        drop(original);
        unsafe {
            libc::close(terminal_read);
            libc::close(unrelated_read);
            libc::close(unrelated_write);
        }
    }

    #[test]
    fn failed_handle_duplication_keeps_the_previous_endpoint_and_closes_the_primary() {
        let backend = UnixPtyBackend::new().expect("backend");
        let (read_end, write_end) = nonblocking_pipe();

        // A previous owner is registered, as on a rerun of a command pane.
        let previous = unsafe { BorrowedFd::borrow_raw(read_end) }
            .try_clone_to_owned()
            .expect("dup previous owner");
        backend.terminal_id_to_raw_fd.lock().unwrap().insert(
            9,
            Some(TerminalEndpoint {
                generation: Some(1),
                fd: Arc::new(previous),
            }),
        );
        // Another terminal's entry must be left alone.
        backend.reserve_terminal_id(10);

        let result = backend.retain_primary_handle(9, write_end, |_| {
            Err(io::Error::from_raw_os_error(libc::EMFILE))
        });
        assert!(result.is_err());

        let registry = backend.terminal_id_to_raw_fd.lock().unwrap();
        assert!(
            registry.contains_key(&9),
            "a failed rerun must leave the previous run addressable"
        );
        assert!(registry.contains_key(&10));
        drop(registry);
        let mut buf = [0u8; 1];
        assert_eq!(
            unsafe { libc::read(read_end, buf.as_mut_ptr() as *mut _, buf.len()) },
            0,
            "the primary write side must be closed"
        );
        unsafe {
            libc::close(read_end);
        }
    }

    #[test]
    fn a_stamped_write_does_not_follow_a_replaced_endpoint() {
        // The writer still considers generation 1 adopted. The PTY owner
        // has already installed generation 2's endpoint. The bytes must
        // not reach that pipe, or the pipe they were originally for.
        let backend = UnixPtyBackend::new().expect("backend");
        let (old_read, old_write) = nonblocking_pipe();
        let (new_read, new_write) = nonblocking_pipe();
        let old_fd = unsafe { OwnedFd::from_raw_fd(old_write) };
        let new_fd = unsafe { OwnedFd::from_raw_fd(new_write) };
        backend.terminal_id_to_raw_fd.lock().unwrap().insert(
            3,
            Some(TerminalEndpoint {
                generation: Some(1),
                fd: Arc::new(old_fd),
            }),
        );
        backend.terminal_id_to_raw_fd.lock().unwrap().insert(
            3,
            Some(TerminalEndpoint {
                generation: Some(2),
                fd: Arc::new(new_fd),
            }),
        );

        let error = backend
            .write_to_tty_stdin_for_run(3, Some(1), b"old-input")
            .expect_err("generation 1 must not write to the replacement");
        assert!(
            error
                .to_string()
                .contains("terminal run changed before the write"),
            "{error}"
        );
        let mut buf = [0u8; 16];
        assert_eq!(
            unsafe { libc::read(new_read, buf.as_mut_ptr() as *mut _, buf.len()) },
            -1,
            "the replacement pipe got bytes"
        );
        let retired = unsafe { libc::read(old_read, buf.as_mut_ptr() as *mut _, buf.len()) };
        assert!(retired <= 0, "the retired pipe got {retired} bytes");

        let written = backend
            .write_to_tty_stdin_for_run(3, Some(2), b"live")
            .expect("generation 2 matches the installed endpoint");
        assert_eq!(written, 4);
        let read = unsafe { libc::read(new_read, buf.as_mut_ptr() as *mut _, buf.len()) };
        assert_eq!(read, 4);
        assert_eq!(&buf[..4], b"live");

        unsafe {
            libc::close(old_read);
            libc::close(new_read);
        }
    }

    #[test]
    fn a_stamped_write_waits_until_the_installed_endpoint_is_bound() {
        let backend = UnixPtyBackend::new().expect("backend");
        let (read_end, write_end) = nonblocking_pipe();
        let owned = unsafe { OwnedFd::from_raw_fd(write_end) };
        backend.terminal_id_to_raw_fd.lock().unwrap().insert(
            4,
            Some(TerminalEndpoint {
                generation: None,
                fd: Arc::new(owned),
            }),
        );
        let error = backend
            .write_to_tty_stdin_for_run(4, Some(1), b"early")
            .expect_err("an unbound endpoint refuses a stamped write");
        assert!(error
            .to_string()
            .contains("terminal run changed before the write"));
        backend.bind_terminal_generation(4, 1);
        let written = backend
            .write_to_tty_stdin_for_run(4, Some(1), b"bound")
            .expect("bound generation accepts the write");
        assert_eq!(written, 5);
        let mut buf = [0u8; 8];
        let read = unsafe { libc::read(read_end, buf.as_mut_ptr() as *mut _, buf.len()) };
        assert_eq!(&buf[..read as usize], b"bound");
        unsafe {
            libc::close(read_end);
        }
    }

    fn nonblocking_pipe() -> (libc::c_int, libc::c_int) {
        let mut fds = [0 as libc::c_int; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        for fd in fds {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            assert!(flags >= 0);
            assert_eq!(
                unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) },
                0
            );
        }
        (fds[0], fds[1])
    }

    #[test]
    fn raw_fd_reader_returns_exact_bytes_in_order() {
        let mut fds = [0 as libc::c_int; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let mut writer = unsafe { File::from_raw_fd(fds[1]) };
        let mut reader = RawFdAsyncReader::new(fds[0]).expect("reader");

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            writer.write_all(b"hello").unwrap();
            let chunk = reader.read_chunk(65536).await.unwrap();
            assert_eq!(chunk, b"hello");
            assert_eq!(chunk.capacity(), chunk.len());

            writer.write_all(b"abc").unwrap();
            writer.write_all(b"def").unwrap();
            writer.write_all(b"ghij").unwrap();
            let mut collected = Vec::new();
            while collected.len() < 10 {
                let chunk = reader.read_chunk(4).await.unwrap();
                assert!(!chunk.is_empty() && chunk.len() <= 4);
                assert_eq!(chunk.capacity(), chunk.len());
                collected.extend_from_slice(&chunk);
            }
            assert_eq!(collected, b"abcdefghij");

            drop(writer);
            let eof = reader.read_chunk(65536).await.unwrap();
            assert!(eof.is_empty());
        });
    }

    /// Verify that `try_write_to_fd` writes as many bytes as the kernel will
    /// accept in one pass and returns a partial count (not an error) when the
    /// PTY buffer fills up.
    ///
    /// A concurrent reader drains the slave side so some bytes are accepted.
    /// The key assertion: the function returns Ok(n) where n <= buf.len(),
    /// and the caller (PtyWriter) is responsible for re-queuing the rest.
    #[test]
    fn try_write_to_fd_returns_partial_on_full_buffer() {
        let pty = openpty(None, &None).expect("openpty failed");
        let master_fd = pty.master.into_raw_fd();
        let slave_fd = pty.slave.into_raw_fd();
        let borrowed_master = unsafe { BorrowedFd::borrow_raw(master_fd) };
        let borrowed_slave = unsafe { BorrowedFd::borrow_raw(slave_fd) };

        let mut attrs = termios::tcgetattr(borrowed_slave).expect("tcgetattr failed");
        termios::cfmakeraw(&mut attrs);
        termios::tcsetattr(borrowed_slave, termios::SetArg::TCSANOW, &attrs)
            .expect("tcsetattr failed");

        // O_NONBLOCK so write() returns EAGAIN instead of blocking
        let flags = fcntl(borrowed_master, FcntlArg::F_GETFL).expect("F_GETFL");
        let mut oflags = OFlag::from_bits_truncate(flags);
        oflags.insert(OFlag::O_NONBLOCK);
        fcntl(borrowed_master, FcntlArg::F_SETFL(oflags)).expect("F_SETFL");

        // Fill most of the buffer, leaving some space
        let chunk = vec![0x42u8; 1024];
        let mut total_filled = 0;
        loop {
            match super::try_write_to_fd(master_fd, &chunk) {
                Ok(0) => break,
                Ok(n) => total_filled += n,
                Err(e) => panic!("unexpected error filling buffer: {e:?}"),
            }
        }
        assert!(
            total_filled > 0,
            "should have written some bytes to fill buffer"
        );

        // Read a small amount from the slave to free partial space
        let mut drain = vec![0u8; 512];
        let slave_file = unsafe { std::fs::File::from_raw_fd(slave_fd) };
        let mut slave_reader = std::io::BufReader::new(&slave_file);
        let drained = slave_reader.read(&mut drain).expect("slave read failed");
        assert!(drained > 0, "should have drained some bytes");
        // Prevent File from closing the slave fd — we close it manually below
        std::mem::forget(slave_file);

        // Now write more than the freed space — should get a partial write
        let size = 128 * 1024;
        let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
        let written = super::try_write_to_fd(master_fd, &data)
            .expect("try_write_to_fd should not error on EAGAIN");

        assert!(
            written > 0 && written < size,
            "expected partial write, got {written}/{size}",
        );

        unsafe {
            libc::close(master_fd);
            libc::close(slave_fd);
        }
    }

    #[test]
    fn write_bounded_stops_after_the_syscall_budget() {
        let buf = vec![0u8; 1024 * 1024];

        // A consumer taking 1 KiB per call.
        let mut calls = 0;
        let written = write_bounded(&buf, 4, |_| {
            calls += 1;
            Ok(1024)
        })
        .unwrap();
        assert_eq!((calls, written), (4, 4096));

        // EINTR retries spend budget too.
        let mut calls = 0;
        let written = write_bounded(&buf, 4, |_| {
            calls += 1;
            Err(nix::errno::Errno::EINTR)
        })
        .unwrap();
        assert_eq!((calls, written), (4, 0));

        // EAGAIN still ends the call early, and errors still propagate.
        let mut calls = 0;
        let written = write_bounded(&buf, 4, |_| {
            calls += 1;
            if calls == 1 {
                Ok(10)
            } else {
                Err(nix::errno::Errno::EAGAIN)
            }
        })
        .unwrap();
        assert_eq!((calls, written), (2, 10));
        assert!(write_bounded(&buf, 4, |_| Err(nix::errno::Errno::EBADF)).is_err());
    }

    #[test]
    fn write_bounded_reports_the_prefix_before_a_fatal_error() {
        let mut calls = 0;
        let result = write_bounded(b"abcdef", 4, |_| {
            calls += 1;
            if calls == 1 {
                Ok(2)
            } else {
                Err(nix::errno::Errno::EIO)
            }
        });

        match result {
            Err((written, error)) => {
                assert_eq!(written, 2);
                assert_eq!(error, nix::errno::Errno::EIO);
            },
            Ok(written) => panic!("expected a fatal error after 2 bytes, wrote {written}"),
        }
    }

    /// Verify that `try_write_to_fd` returns Ok(0) — not an error — when the
    /// fd is completely full and cannot accept any bytes at all.
    #[test]
    fn try_write_to_fd_returns_zero_on_stuck_pty() {
        let pty = openpty(None, &None).expect("openpty failed");
        let master_fd = pty.master.into_raw_fd();
        let slave_fd = pty.slave.into_raw_fd();
        let borrowed_master = unsafe { BorrowedFd::borrow_raw(master_fd) };
        let borrowed_slave = unsafe { BorrowedFd::borrow_raw(slave_fd) };

        let mut attrs = termios::tcgetattr(borrowed_slave).expect("tcgetattr failed");
        termios::cfmakeraw(&mut attrs);
        termios::tcsetattr(borrowed_slave, termios::SetArg::TCSANOW, &attrs)
            .expect("tcsetattr failed");

        let flags = fcntl(borrowed_master, FcntlArg::F_GETFL).expect("F_GETFL");
        let mut oflags = OFlag::from_bits_truncate(flags);
        oflags.insert(OFlag::O_NONBLOCK);
        fcntl(borrowed_master, FcntlArg::F_SETFL(oflags)).expect("F_SETFL");

        // Fill the buffer completely — keep writing until we get Ok(0)
        let fill = vec![0x42u8; 1024];
        loop {
            match super::try_write_to_fd(master_fd, &fill) {
                Ok(0) => break,
                Ok(_) => continue,
                Err(e) => panic!("unexpected error filling buffer: {e:?}"),
            }
        }

        // Now the buffer is full — next write should return Ok(0)
        let written = super::try_write_to_fd(master_fd, &[0x01, 0x02, 0x03])
            .expect("try_write_to_fd should not error on EAGAIN");

        assert_eq!(written, 0, "expected zero bytes written on full buffer");

        unsafe {
            libc::close(master_fd);
            libc::close(slave_fd);
        }
    }

    #[test]
    fn initial_winsize_defaults_to_nonzero_geometry() {
        let backend = UnixPtyBackend::new().expect("backend");
        let winsize = backend.initial_winsize_for(1);
        assert!(winsize.ws_col > 0 && winsize.ws_row > 0);
        assert_eq!((winsize.ws_col, winsize.ws_row), (80, 24));
    }

    #[test]
    fn set_terminal_size_remembers_geometry_for_the_next_spawn() {
        let backend = UnixPtyBackend::new().expect("backend");
        backend
            .set_terminal_size(3, 120, 40, Some(960), Some(640))
            .expect("remember size without an open endpoint");
        let winsize = backend.initial_winsize_for(3);
        assert_eq!(
            (winsize.ws_col, winsize.ws_row, winsize.ws_xpixel, winsize.ws_ypixel),
            (120, 40, 960, 640)
        );

        backend.clear_terminal_id(3);
        let reset = backend.initial_winsize_for(3);
        assert_eq!((reset.ws_col, reset.ws_row), (80, 24));
    }

    #[test]
    fn openpty_is_seeded_with_the_remembered_nonzero_winsize() {
        let backend = UnixPtyBackend::new().expect("backend");
        backend
            .set_terminal_size(11, 100, 32, None, None)
            .expect("seed remembered size");
        let winsize = backend.initial_winsize_for(11);
        let pty = openpty(Some(&winsize), &None).expect("openpty");
        let master_fd = pty.master.into_raw_fd();
        let slave_fd = pty.slave.into_raw_fd();
        let mut readback = Winsize {
            ws_row: 0,
            ws_col: 0,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        #[allow(clippy::useless_conversion)]
        let rc = unsafe { ioctl(master_fd, TIOCGWINSZ.into(), &mut readback) };
        assert_eq!(rc, 0, "TIOCGWINSZ must succeed");
        assert_eq!((readback.ws_col, readback.ws_row), (100, 32));
        unsafe {
            libc::close(master_fd);
            libc::close(slave_fd);
        }
    }
}

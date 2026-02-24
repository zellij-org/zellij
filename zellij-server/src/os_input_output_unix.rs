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

        match self
            .terminal_id_to_raw_fd
            .lock()
            .to_anyhow()
            .with_context(err_context)?
            .get(&terminal_id)
        {
            Some(Some(fd)) => unistd::write(*fd, buf).with_context(err_context),
            _ => Err(anyhow!("could not find raw file descriptor")).with_context(err_context),
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

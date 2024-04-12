use crate::{panes::PaneId, ClientId};

use async_std::{fs::File as AsyncFile, io::ReadExt, os::unix::io::FromRawFd};
use interprocess::local_socket::LocalSocketStream;
use nix::{
    pty::{openpty, OpenptyResult, Winsize},
    sys::{
        signal::{kill, Signal},
        termios,
    },
    unistd,
};

use signal_hook::consts::*;
use sysinfo::{ProcessExt, ProcessRefreshKind, System, SystemExt};
use zellij_utils::{
    async_std, channels,
    channels::TrySendError,
    data::Palette,
    errors::prelude::*,
    input::command::{RunCommand, TerminalAction},
    interprocess,
    ipc::{
        ClientToServerMsg, ExitReason, IpcReceiverWithContext, IpcSenderWithContext,
        ServerToClientMsg,
    },
    libc, nix,
    shared::default_palette,
    signal_hook,
    tempfile::tempfile,
};

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    env,
    fs::File,
    io::Write,
    os::unix::{io::RawFd, process::CommandExt},
    path::PathBuf,
    process::{Child, Command},
    sync::{Arc, Mutex},
};

pub use async_trait::async_trait;
pub use nix::unistd::Pid;

fn set_terminal_size_using_fd(
    fd: RawFd,
    columns: u16,
    rows: u16,
    width_in_pixels: Option<u16>,
    height_in_pixels: Option<u16>,
) {
    // TODO: do this with the nix ioctl
    use libc::ioctl;
    use libc::TIOCSWINSZ;

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
                ::std::thread::sleep(::std::time::Duration::from_millis(10));
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
            kill(Pid::from_raw(child.id() as i32), Some(Signal::SIGTERM))
                .with_context(err_context)?;
            continue;
        } else {
            // when I say whoa, I mean WHOA!
            let _ = child.kill();
            break 'handle_exit Ok(None);
        }
    }
}

fn command_exists(cmd: &RunCommand) -> bool {
    let command = &cmd.command;
    match cmd.cwd.as_ref() {
        Some(cwd) => {
            let full_command = cwd.join(&command);
            if full_command.exists() && full_command.is_file() {
                return true;
            }
        },
        None => {
            if command.exists() && command.is_file() {
                return true;
            }
        },
    }

    if let Some(paths) = env::var_os("PATH") {
        for path in env::split_paths(&paths) {
            let full_command = path.join(command);
            if full_command.exists() && full_command.is_file() {
                return true;
            }
        }
    }
    false
}

fn handle_openpty(
    open_pty_res: OpenptyResult,
    cmd: RunCommand,
    quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>, // u32 is the exit status
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

    if command_exists(&cmd) {
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
                .pre_exec(move || -> std::io::Result<()> {
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
        std::thread::spawn(move || {
            child.wait().with_context(|| err_context(&cmd)).fatal();
            let exit_status = handle_command_exit(child)
                .with_context(|| err_context(&cmd))
                .fatal();
            let _ = nix::unistd::close(pid_secondary);
            quit_cb(PaneId::Terminal(terminal_id), exit_status, cmd);
        });

        Ok((pid_primary, child_id as RawFd))
    } else {
        Err(ZellijError::CommandNotFound {
            terminal_id,
            command: cmd.command.to_string_lossy().to_string(),
        })
        .with_context(|| err_context(&cmd))
    }
}

/// Spawns a new terminal from the parent terminal with [`termios`](termios::Termios)
/// `orig_termios`.
///
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

// this is a utility method to separate the arguments from a pathbuf before we turn it into a
// Command. eg. "/usr/bin/vim -e" ==> "/usr/bin/vim" + "-e" (the latter will be pushed to args)
fn separate_command_arguments(command: &mut PathBuf, args: &mut Vec<String>) {
    let mut parts = vec![];
    let mut current_part = String::new();
    for part in command.display().to_string().split_ascii_whitespace() {
        current_part.push_str(part);
        if current_part.ends_with('\\') {
            let _ = current_part.pop();
            current_part.push(' ');
        } else {
            let current_part = std::mem::replace(&mut current_part, String::new());
            parts.push(current_part);
        }
    }
    if !parts.is_empty() {
        *command = PathBuf::from(parts.remove(0));
        args.append(&mut parts);
    }
}

/// If a [`TerminalAction::OpenFile(file)`] is given, the text editor specified by environment variable `EDITOR`
/// (or `VISUAL`, if `EDITOR` is not set) will be started in the new terminal, with the given
/// file open.
/// If [`TerminalAction::RunCommand(RunCommand)`] is given, the command will be started
/// in the new terminal.
/// If None is given, the shell specified by environment variable `SHELL` will
/// be started in the new terminal.
///
/// # Panics
///
/// This function will panic if both the `EDITOR` and `VISUAL` environment variables are not
/// set.
fn spawn_terminal(
    terminal_action: TerminalAction,
    orig_termios: Option<termios::Termios>,
    quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>, // u32 is the exit_status
    default_editor: Option<PathBuf>,
    terminal_id: u32,
) -> Result<(RawFd, RawFd)> {
    // returns the terminal_id, the primary fd and the
    // secondary fd
    let mut failover_cmd_args = None;
    let cmd = match terminal_action {
        TerminalAction::OpenFile(mut file_to_open, line_number, cwd) => {
            if file_to_open.is_relative() {
                if let Some(cwd) = cwd.as_ref() {
                    file_to_open = cwd.join(file_to_open);
                }
            }
            let mut command = default_editor.unwrap_or_else(|| {
                PathBuf::from(
                    env::var("EDITOR")
                        .unwrap_or_else(|_| env::var("VISUAL").unwrap_or_else(|_| "vi".into())),
                )
            });

            let mut args = vec![];

            if !command.is_dir() {
                separate_command_arguments(&mut command, &mut args);
            }
            let file_to_open = file_to_open
                .into_os_string()
                .into_string()
                .expect("Not valid Utf8 Encoding");
            if let Some(line_number) = line_number {
                if command.ends_with("vim")
                    || command.ends_with("nvim")
                    || command.ends_with("emacs")
                    || command.ends_with("nano")
                    || command.ends_with("kak")
                {
                    failover_cmd_args = Some(vec![file_to_open.clone()]);
                    args.push(format!("+{}", line_number));
                    args.push(file_to_open);
                } else if command.ends_with("hx") || command.ends_with("helix") {
                    // at the time of writing, helix only supports this syntax
                    // and it might be a good idea to leave this here anyway
                    // to keep supporting old versions
                    args.push(format!("{}:{}", file_to_open, line_number));
                } else {
                    args.push(file_to_open);
                }
            } else {
                args.push(file_to_open);
            }
            RunCommand {
                command,
                args,
                cwd,
                hold_on_close: false,
                hold_on_start: false,
            }
        },
        TerminalAction::RunCommand(command) => command,
    };
    let failover_cmd = if let Some(failover_cmd_args) = failover_cmd_args {
        let mut cmd = cmd.clone();
        cmd.args = failover_cmd_args;
        Some(cmd)
    } else {
        None
    };

    handle_terminal(cmd, failover_cmd, orig_termios, quit_cb, terminal_id)
}

// The ClientSender is in charge of sending messages to the client on a special thread
// This is done so that when the unix socket buffer is full, we won't block the entire router
// thread
// When the above happens, the ClientSender buffers messages in hopes that the congestion will be
// freed until we runs out of buffer space.
// If we run out of buffer space, we bubble up an error sot hat the router thread will give up on
// this client and we'll stop sending messages to it.
// If the client ever becomes responsive again, we'll send one final "Buffer full" message so it
// knows what happened.
#[derive(Clone)]
struct ClientSender {
    client_id: ClientId,
    client_buffer_sender: channels::Sender<ServerToClientMsg>,
}

impl ClientSender {
    pub fn new(client_id: ClientId, mut sender: IpcSenderWithContext<ServerToClientMsg>) -> Self {
        // FIXME(hartan): This queue is responsible for buffering messages between server and
        // client. If it fills up, the client is disconnected with a "Buffer full" sort of error
        // message. It was previously found to be too small (with depth 50), so it was increased to
        // 5000 instead. This decision was made because it was found that a queue of depth 5000
        // doesn't cause noticable increase in RAM usage, but there's no reason beyond that. If in
        // the future this is found to fill up too quickly again, it may be worthwhile to increase
        // the size even further (or better yet, implement a redraw-on-backpressure mechanism).
        // We, the zellij maintainers, have decided against an unbounded
        // queue for the time being because we want to prevent e.g. the whole session being killed
        // (by OOM-killers or some other mechanism) just because a single client doesn't respond.
        let (client_buffer_sender, client_buffer_receiver) = channels::bounded(5000);
        std::thread::spawn(move || {
            let err_context = || format!("failed to send message to client {client_id}");
            for msg in client_buffer_receiver.iter() {
                sender.send(msg).with_context(err_context).non_fatal();
            }
            // If we're here, the message buffer is broken for some reason
            let _ = sender.send(ServerToClientMsg::Exit(ExitReason::Disconnect));
        });
        ClientSender {
            client_id,
            client_buffer_sender,
        }
    }
    pub fn send_or_buffer(&self, msg: ServerToClientMsg) -> Result<()> {
        let err_context = || {
            format!(
                "failed to send or buffer message for client {}",
                self.client_id
            )
        };

        self.client_buffer_sender
            .try_send(msg)
            .or_else(|err| {
                if let TrySendError::Full(_) = err {
                    log::warn!(
                        "client {} is processing server messages too slow",
                        self.client_id
                    );
                }
                Err(err)
            })
            .with_context(err_context)
    }
}

#[derive(Clone)]
pub struct ServerOsInputOutput {
    orig_termios: Arc<Mutex<Option<termios::Termios>>>,
    client_senders: Arc<Mutex<HashMap<ClientId, ClientSender>>>,
    terminal_id_to_raw_fd: Arc<Mutex<BTreeMap<u32, Option<RawFd>>>>, // A value of None means the
    // terminal_id exists but is
    // not connected to an fd (eg.
    // a command pane with a
    // non-existing command)
    cached_resizes: Arc<Mutex<Option<BTreeMap<u32, (u16, u16, Option<u16>, Option<u16>)>>>>, // <terminal_id, (cols, rows, width_in_pixels, height_in_pixels)>
}

// async fn in traits is not supported by rust, so dtolnay's excellent async_trait macro is being
// used. See https://smallcultfollowing.com/babysteps/blog/2019/10/26/async-fn-in-traits-are-hard/
#[async_trait]
pub trait AsyncReader: Send + Sync {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error>;
}

/// An `AsyncReader` that wraps a `RawFd`
struct RawFdAsyncReader {
    fd: async_std::fs::File,
}

impl RawFdAsyncReader {
    fn new(fd: RawFd) -> RawFdAsyncReader {
        RawFdAsyncReader {
            // The supplied `RawFd` is consumed by the created `RawFdAsyncReader`, closing it when dropped
            fd: unsafe { AsyncFile::from_raw_fd(fd) },
        }
    }
}

#[async_trait]
impl AsyncReader for RawFdAsyncReader {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        self.fd.read(buf).await
    }
}

/// The `ServerOsApi` trait represents an abstract interface to the features of an operating system that
/// Zellij server requires.
pub trait ServerOsApi: Send + Sync {
    fn set_terminal_size_using_terminal_id(
        &self,
        id: u32,
        cols: u16,
        rows: u16,
        width_in_pixels: Option<u16>,
        height_in_pixels: Option<u16>,
    ) -> Result<()>;
    /// Spawn a new terminal, with a terminal action. The returned tuple contains the master file
    /// descriptor of the forked pseudo terminal and a [ChildId] struct containing process id's for
    /// the forked child process.
    fn spawn_terminal(
        &self,
        terminal_action: TerminalAction,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>, // u32 is the exit status
        default_editor: Option<PathBuf>,
    ) -> Result<(u32, RawFd, RawFd)>;
    // reserves a terminal id without actually opening a terminal
    fn reserve_terminal_id(&self) -> Result<u32> {
        unimplemented!()
    }
    /// Read bytes from the standard output of the virtual terminal referred to by `fd`.
    fn read_from_tty_stdout(&self, fd: RawFd, buf: &mut [u8]) -> Result<usize>;
    /// Creates an `AsyncReader` that can be used to read from `fd` in an async context
    fn async_file_reader(&self, fd: RawFd) -> Box<dyn AsyncReader>;
    /// Write bytes to the standard input of the virtual terminal referred to by `fd`.
    fn write_to_tty_stdin(&self, terminal_id: u32, buf: &[u8]) -> Result<usize>;
    /// Wait until all output written to the object referred to by `fd` has been transmitted.
    fn tcdrain(&self, terminal_id: u32) -> Result<()>;
    /// Terminate the process with process ID `pid`. (SIGTERM)
    fn kill(&self, pid: Pid) -> Result<()>;
    /// Terminate the process with process ID `pid`. (SIGKILL)
    fn force_kill(&self, pid: Pid) -> Result<()>;
    /// Returns a [`Box`] pointer to this [`ServerOsApi`] struct.
    fn box_clone(&self) -> Box<dyn ServerOsApi>;
    fn send_to_client(&self, client_id: ClientId, msg: ServerToClientMsg) -> Result<()>;
    fn new_client(
        &mut self,
        client_id: ClientId,
        stream: LocalSocketStream,
    ) -> Result<IpcReceiverWithContext<ClientToServerMsg>>;
    fn remove_client(&mut self, client_id: ClientId) -> Result<()>;
    fn load_palette(&self) -> Palette;
    /// Returns the current working directory for a given pid
    fn get_cwd(&self, pid: Pid) -> Option<PathBuf>;
    /// Returns the current working directory for multiple pids
    fn get_cwds(&self, _pids: Vec<Pid>) -> HashMap<Pid, PathBuf> {
        HashMap::new()
    }
    /// Get a list of all running commands by their parent process id
    fn get_all_cmds_by_ppid(&self) -> HashMap<String, Vec<String>> {
        HashMap::new()
    }
    /// Writes the given buffer to a string
    fn write_to_file(&mut self, buf: String, file: Option<String>) -> Result<()>;

    fn re_run_command_in_terminal(
        &self,
        terminal_id: u32,
        run_command: RunCommand,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>, // u32 is the exit status
    ) -> Result<(RawFd, RawFd)>;
    fn clear_terminal_id(&self, terminal_id: u32) -> Result<()>;
    fn cache_resizes(&mut self) {}
    fn apply_cached_resizes(&mut self) {}
}

impl ServerOsApi for ServerOsInputOutput {
    fn set_terminal_size_using_terminal_id(
        &self,
        id: u32,
        cols: u16,
        rows: u16,
        width_in_pixels: Option<u16>,
        height_in_pixels: Option<u16>,
    ) -> Result<()> {
        let err_context = || {
            format!(
                "failed to set terminal id {} to size ({}, {})",
                id, rows, cols
            )
        };
        if let Some(cached_resizes) = self.cached_resizes.lock().unwrap().as_mut() {
            cached_resizes.insert(id, (cols, rows, width_in_pixels, height_in_pixels));
            return Ok(());
        }

        match self
            .terminal_id_to_raw_fd
            .lock()
            .to_anyhow()
            .with_context(err_context)?
            .get(&id)
        {
            Some(Some(fd)) => {
                if cols > 0 && rows > 0 {
                    set_terminal_size_using_fd(*fd, cols, rows, width_in_pixels, height_in_pixels);
                }
            },
            _ => {
                Err::<(), _>(anyhow!("failed to find terminal fd for id {id}"))
                    .with_context(err_context)
                    .non_fatal();
            },
        }
        Ok(())
    }
    #[allow(unused_assignments)]
    fn spawn_terminal(
        &self,
        terminal_action: TerminalAction,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>, // u32 is the exit status
        default_editor: Option<PathBuf>,
    ) -> Result<(u32, RawFd, RawFd)> {
        let err_context = || "failed to spawn terminal".to_string();

        let orig_termios = self
            .orig_termios
            .lock()
            .to_anyhow()
            .with_context(err_context)?;
        let terminal_id = self
            .terminal_id_to_raw_fd
            .lock()
            .to_anyhow()
            .with_context(err_context)?
            .keys()
            .copied()
            .collect::<BTreeSet<u32>>()
            .last()
            .map(|l| l + 1)
            .or(Some(0));
        match terminal_id {
            Some(terminal_id) => {
                self.terminal_id_to_raw_fd
                    .lock()
                    .to_anyhow()
                    .with_context(err_context)?
                    .insert(terminal_id, None);
                spawn_terminal(
                    terminal_action,
                    orig_termios.clone(),
                    quit_cb,
                    default_editor,
                    terminal_id,
                )
                .and_then(|(pid_primary, pid_secondary)| {
                    self.terminal_id_to_raw_fd
                        .lock()
                        .to_anyhow()?
                        .insert(terminal_id, Some(pid_primary));
                    Ok((terminal_id, pid_primary, pid_secondary))
                })
                .with_context(err_context)
            },
            None => Err(anyhow!("no more terminal IDs left to allocate")),
        }
    }
    #[allow(unused_assignments)]
    fn reserve_terminal_id(&self) -> Result<u32> {
        let err_context = || "failed to reserve a terminal ID".to_string();

        let terminal_id = self
            .terminal_id_to_raw_fd
            .lock()
            .to_anyhow()
            .with_context(err_context)?
            .keys()
            .copied()
            .collect::<BTreeSet<u32>>()
            .last()
            .map(|l| l + 1)
            .or(Some(0));
        match terminal_id {
            Some(terminal_id) => {
                self.terminal_id_to_raw_fd
                    .lock()
                    .to_anyhow()
                    .with_context(err_context)?
                    .insert(terminal_id, None);
                Ok(terminal_id)
            },
            None => Err(anyhow!("no more terminal IDs available")),
        }
    }
    fn read_from_tty_stdout(&self, fd: RawFd, buf: &mut [u8]) -> Result<usize> {
        unistd::read(fd, buf).with_context(|| format!("failed to read stdout of raw FD {}", fd))
    }
    fn async_file_reader(&self, fd: RawFd) -> Box<dyn AsyncReader> {
        Box::new(RawFdAsyncReader::new(fd))
    }
    fn write_to_tty_stdin(&self, terminal_id: u32, buf: &[u8]) -> Result<usize> {
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
    fn tcdrain(&self, terminal_id: u32) -> Result<()> {
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
    fn box_clone(&self) -> Box<dyn ServerOsApi> {
        Box::new((*self).clone())
    }
    fn kill(&self, pid: Pid) -> Result<()> {
        let _ = kill(pid, Some(Signal::SIGHUP));
        Ok(())
    }
    fn force_kill(&self, pid: Pid) -> Result<()> {
        let _ = kill(pid, Some(Signal::SIGKILL));
        Ok(())
    }
    fn send_to_client(&self, client_id: ClientId, msg: ServerToClientMsg) -> Result<()> {
        let err_context = || format!("failed to send message to client {client_id}");

        if let Some(sender) = self
            .client_senders
            .lock()
            .to_anyhow()
            .with_context(err_context)?
            .get_mut(&client_id)
        {
            sender.send_or_buffer(msg).with_context(err_context)
        } else {
            Ok(())
        }
    }

    fn new_client(
        &mut self,
        client_id: ClientId,
        stream: LocalSocketStream,
    ) -> Result<IpcReceiverWithContext<ClientToServerMsg>> {
        let receiver = IpcReceiverWithContext::new(stream);
        let sender = ClientSender::new(client_id, receiver.get_sender());
        self.client_senders
            .lock()
            .to_anyhow()
            .with_context(|| format!("failed to create new client {client_id}"))?
            .insert(client_id, sender);
        Ok(receiver)
    }

    fn remove_client(&mut self, client_id: ClientId) -> Result<()> {
        let mut client_senders = self
            .client_senders
            .lock()
            .to_anyhow()
            .with_context(|| format!("failed to remove client {client_id}"))?;
        if client_senders.contains_key(&client_id) {
            client_senders.remove(&client_id);
        }
        Ok(())
    }

    fn load_palette(&self) -> Palette {
        default_palette()
    }

    fn get_cwd(&self, pid: Pid) -> Option<PathBuf> {
        let mut system_info = System::new();
        // Update by minimizing information.
        // See https://docs.rs/sysinfo/0.22.5/sysinfo/struct.ProcessRefreshKind.html#
        system_info.refresh_processes_specifics(ProcessRefreshKind::default());

        if let Some(process) = system_info.process(pid.into()) {
            let cwd = process.cwd();
            let cwd_is_empty = cwd.iter().next().is_none();
            if !cwd_is_empty {
                return Some(process.cwd().to_path_buf());
            }
        }
        None
    }

    fn get_cwds(&self, pids: Vec<Pid>) -> HashMap<Pid, PathBuf> {
        let mut system_info = System::new();
        let mut cwds = HashMap::new();

        for pid in pids {
            // Update by minimizing information.
            // See https://docs.rs/sysinfo/0.22.5/sysinfo/struct.ProcessRefreshKind.html#
            let is_found =
                system_info.refresh_process_specifics(pid.into(), ProcessRefreshKind::default());
            if is_found {
                if let Some(process) = system_info.process(pid.into()) {
                    let cwd = process.cwd();
                    let cwd_is_empty = cwd.iter().next().is_none();
                    if !cwd_is_empty {
                        cwds.insert(pid, process.cwd().to_path_buf());
                    }
                }
            }
        }

        cwds
    }
    fn get_all_cmds_by_ppid(&self) -> HashMap<String, Vec<String>> {
        // the key is the stringified ppid
        let mut cmds = HashMap::new();
        if let Some(output) = Command::new("ps")
            .args(vec!["-ao", "ppid,args"])
            .output()
            .ok()
        {
            let output = String::from_utf8(output.stdout.clone())
                .unwrap_or_else(|_| String::from_utf8_lossy(&output.stdout).to_string());
            for line in output.lines() {
                let line_parts: Vec<String> = line
                    .trim()
                    .split_ascii_whitespace()
                    .map(|p| p.to_owned())
                    .collect();
                let mut line_parts = line_parts.into_iter();
                let ppid = line_parts.next();
                if let Some(ppid) = ppid {
                    cmds.insert(ppid.into(), line_parts.collect());
                }
            }
        }
        cmds
    }

    fn write_to_file(&mut self, buf: String, name: Option<String>) -> Result<()> {
        let err_context = || "failed to write to file".to_string();

        let mut f: File = match name {
            Some(x) => File::create(x).with_context(err_context)?,
            None => tempfile().with_context(err_context)?,
        };
        write!(f, "{}", buf).with_context(err_context)
    }

    fn re_run_command_in_terminal(
        &self,
        terminal_id: u32,
        run_command: RunCommand,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>, // u32 is the exit status
    ) -> Result<(RawFd, RawFd)> {
        let default_editor = None; // no need for a default editor when running an explicit command
        self.orig_termios
            .lock()
            .to_anyhow()
            .and_then(|orig_termios| {
                spawn_terminal(
                    TerminalAction::RunCommand(run_command),
                    orig_termios.clone(),
                    quit_cb,
                    default_editor,
                    terminal_id,
                )
            })
            .and_then(|(pid_primary, pid_secondary)| {
                self.terminal_id_to_raw_fd
                    .lock()
                    .to_anyhow()?
                    .insert(terminal_id, Some(pid_primary));
                Ok((pid_primary, pid_secondary))
            })
            .with_context(|| format!("failed to rerun command in terminal id {}", terminal_id))
    }
    fn clear_terminal_id(&self, terminal_id: u32) -> Result<()> {
        self.terminal_id_to_raw_fd
            .lock()
            .to_anyhow()
            .with_context(|| format!("failed to clear terminal ID {}", terminal_id))?
            .remove(&terminal_id);
        Ok(())
    }
    fn cache_resizes(&mut self) {
        if self.cached_resizes.lock().unwrap().is_none() {
            *self.cached_resizes.lock().unwrap() = Some(BTreeMap::new());
        }
    }
    fn apply_cached_resizes(&mut self) {
        let mut cached_resizes = self.cached_resizes.lock().unwrap().take();
        if let Some(cached_resizes) = cached_resizes.as_mut() {
            for (terminal_id, (cols, rows, width_in_pixels, height_in_pixels)) in
                cached_resizes.iter()
            {
                let _ = self.set_terminal_size_using_terminal_id(
                    *terminal_id,
                    *cols,
                    *rows,
                    width_in_pixels.clone(),
                    height_in_pixels.clone(),
                );
            }
        }
    }
}

impl Clone for Box<dyn ServerOsApi> {
    fn clone(&self) -> Box<dyn ServerOsApi> {
        self.box_clone()
    }
}

pub fn get_server_os_input() -> Result<ServerOsInputOutput, nix::Error> {
    let current_termios = termios::tcgetattr(0).ok();
    if current_termios.is_none() {
        log::warn!("Starting a server without a controlling terminal, using the default termios configuration.");
    }
    let orig_termios = Arc::new(Mutex::new(current_termios));
    Ok(ServerOsInputOutput {
        orig_termios,
        client_senders: Arc::new(Mutex::new(HashMap::new())),
        terminal_id_to_raw_fd: Arc::new(Mutex::new(BTreeMap::new())),
        cached_resizes: Arc::new(Mutex::new(None)),
    })
}

use crate::pty_writer::PtyWriteInstruction;
use crate::thread_bus::ThreadSenders;

pub struct ResizeCache {
    senders: ThreadSenders,
}

impl ResizeCache {
    pub fn new(senders: ThreadSenders) -> Self {
        senders
            .send_to_pty_writer(PtyWriteInstruction::StartCachingResizes)
            .unwrap_or_else(|e| {
                log::error!("Failed to cache resizes: {}", e);
            });
        ResizeCache { senders }
    }
}

impl Drop for ResizeCache {
    fn drop(&mut self) {
        self.senders
            .send_to_pty_writer(PtyWriteInstruction::ApplyCachedResizes)
            .unwrap_or_else(|e| {
                log::error!("Failed to apply cached resizes: {}", e);
            });
    }
}

/// Process id's for forked terminals
#[derive(Debug)]
pub struct ChildId {
    /// Primary process id of a forked terminal
    pub primary: Pid,
    /// Process id of the command running inside the forked terminal, usually a shell. The primary
    /// field is it's parent process id.
    pub shell: Option<Pid>,
}

#[cfg(test)]
#[path = "./unit/os_input_output_tests.rs"]
mod os_input_output_tests;

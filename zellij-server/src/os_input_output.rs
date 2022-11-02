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
    async_std,
    data::Palette,
    errors::prelude::*,
    input::command::{RunCommand, TerminalAction},
    interprocess,
    ipc::{ClientToServerMsg, IpcReceiverWithContext, IpcSenderWithContext, ServerToClientMsg},
    libc, nix,
    shared::default_palette,
    signal_hook,
    tempfile::tempfile,
};

use std::{
    collections::{BTreeMap, HashMap, HashSet},
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

fn set_terminal_size_using_fd(fd: RawFd, columns: u16, rows: u16) {
    // TODO: do this with the nix ioctl
    use libc::ioctl;
    use libc::TIOCSWINSZ;

    let winsize = Winsize {
        ws_col: columns,
        ws_row: rows,
        ws_xpixel: 0,
        ws_ypixel: 0,
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
                    // TODO: propagate this to the user
                    return Err(anyhow!(
                        "Failed to set CWD for new pane. '{}' does not exist or is not a folder",
                        current_dir.display()
                    ))
                    .context("failed to open PTY");
                }
            }
            command
                .args(&cmd.args)
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
    orig_termios: termios::Termios,
    quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
    terminal_id: u32,
) -> Result<(RawFd, RawFd)> {
    let err_context = || "failed to spawn child terminal".to_string();

    // Create a pipe to allow the child the communicate the shell's pid to its
    // parent.
    match openpty(None, Some(&orig_termios)) {
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
    if let Some(file_name) = command
        .file_name()
        .and_then(|f_n| f_n.to_str())
        .map(|f_n| f_n.to_string())
    {
        let mut file_name_parts = file_name.split_ascii_whitespace();
        if let Some(first_part) = file_name_parts.next() {
            command.set_file_name(first_part);
            for part in file_name_parts {
                args.push(String::from(part));
            }
        }
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
    orig_termios: termios::Termios,
    quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>, // u32 is the exit_status
    default_editor: Option<PathBuf>,
    terminal_id: u32,
) -> Result<(RawFd, RawFd)> {
    // returns the terminal_id, the primary fd and the
    // secondary fd
    let mut failover_cmd_args = None;
    let cmd = match terminal_action {
        TerminalAction::OpenFile(file_to_open, line_number) => {
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
                }
            }
            args.push(file_to_open);
            RunCommand {
                command,
                args,
                cwd: None,
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

#[derive(Clone)]
pub struct ServerOsInputOutput {
    orig_termios: Arc<Mutex<termios::Termios>>,
    client_senders: Arc<Mutex<HashMap<ClientId, IpcSenderWithContext<ServerToClientMsg>>>>,
    terminal_id_to_raw_fd: Arc<Mutex<BTreeMap<u32, Option<RawFd>>>>, // A value of None means the
                                                                     // terminal_id exists but is
                                                                     // not connected to an fd (eg.
                                                                     // a command pane with a
                                                                     // non-existing command)
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
            /// The supplied `RawFd` is consumed by the created `RawFdAsyncReader`, closing it when dropped
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
    fn set_terminal_size_using_terminal_id(&self, id: u32, cols: u16, rows: u16) -> Result<()>;
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
    /// Writes the given buffer to a string
    fn write_to_file(&mut self, buf: String, file: Option<String>) -> Result<()>;

    fn re_run_command_in_terminal(
        &self,
        terminal_id: u32,
        run_command: RunCommand,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>, // u32 is the exit status
    ) -> Result<(RawFd, RawFd)>;
    fn clear_terminal_id(&self, terminal_id: u32) -> Result<()>;
}

impl ServerOsApi for ServerOsInputOutput {
    fn set_terminal_size_using_terminal_id(&self, id: u32, cols: u16, rows: u16) -> Result<()> {
        let err_context = || {
            format!(
                "failed to set terminal id {} to size ({}, {})",
                id, rows, cols
            )
        };

        match self
            .terminal_id_to_raw_fd
            .lock()
            .to_anyhow()
            .with_context(err_context)?
            .get(&id)
        {
            Some(Some(fd)) => {
                if cols > 0 && rows > 0 {
                    set_terminal_size_using_fd(*fd, cols, rows);
                }
            },
            _ => {
                return Err(anyhow!("failed to find terminal fd for id {id}"))
                    .with_context(err_context);
            },
        }
        Ok(())
    }
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
        let mut terminal_id = None;
        {
            let current_ids: HashSet<u32> = self
                .terminal_id_to_raw_fd
                .lock()
                .to_anyhow()
                .with_context(err_context)?
                .keys()
                .copied()
                .collect();
            for i in 0..u32::MAX {
                let i = i as u32;
                if !current_ids.contains(&i) {
                    terminal_id = Some(i);
                    break;
                }
            }
        }
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
    fn reserve_terminal_id(&self) -> Result<u32> {
        let err_context = || "failed to reserve a terminal ID".to_string();

        let mut terminal_id = None;
        {
            let current_ids: HashSet<u32> = self
                .terminal_id_to_raw_fd
                .lock()
                .to_anyhow()
                .with_context(err_context)?
                .keys()
                .copied()
                .collect();
            for i in 0..u32::MAX {
                let i = i as u32;
                if !current_ids.contains(&i) {
                    terminal_id = Some(i);
                    break;
                }
            }
        }
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
            sender.send(msg).with_context(err_context)
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
        let sender = receiver.get_sender();
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
            return Some(process.cwd().to_path_buf());
        }
        None
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
}

impl Clone for Box<dyn ServerOsApi> {
    fn clone(&self) -> Box<dyn ServerOsApi> {
        self.box_clone()
    }
}

pub fn get_server_os_input() -> Result<ServerOsInputOutput, nix::Error> {
    let current_termios = termios::tcgetattr(0)?;
    let orig_termios = Arc::new(Mutex::new(current_termios));
    Ok(ServerOsInputOutput {
        orig_termios,
        client_senders: Arc::new(Mutex::new(HashMap::new())),
        terminal_id_to_raw_fd: Arc::new(Mutex::new(BTreeMap::new())),
    })
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

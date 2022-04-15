use std::collections::HashMap;

use crate::panes::PaneId;

use std::env;
use std::os::unix::io::RawFd;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};

use zellij_utils::{async_std, interprocess, libc, nix, signal_hook, zellij_tile};

use async_std::fs::File as AsyncFile;
use async_std::os::unix::io::FromRawFd;
use interprocess::local_socket::LocalSocketStream;

use sysinfo::{ProcessExt, ProcessRefreshKind, System, SystemExt};

use nix::pty::{openpty, OpenptyResult, Winsize};
use nix::sys::signal::{kill, Signal};
use nix::sys::termios;

use nix::unistd;
use signal_hook::consts::*;
use zellij_tile::data::Palette;
use zellij_utils::{
    input::command::{RunCommand, TerminalAction},
    ipc::{ClientToServerMsg, IpcReceiverWithContext, IpcSenderWithContext, ServerToClientMsg},
    shared::default_palette,
};

use async_std::io::ReadExt;
pub use async_trait::async_trait;

pub use nix::unistd::Pid;

use crate::ClientId;

pub(crate) fn set_terminal_size_using_fd(fd: RawFd, columns: u16, rows: u16) {
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
fn handle_command_exit(mut child: Child) {
    let mut should_exit = false;
    let mut attempts = 3;
    let mut signals = signal_hook::iterator::Signals::new(&[SIGINT, SIGTERM]).unwrap();
    'handle_exit: loop {
        // test whether the child process has exited
        match child.try_wait() {
            Ok(Some(_status)) => {
                // if the child process has exited, break outside of the loop
                // and exit this function
                // TODO: handle errors?
                break 'handle_exit;
            }
            Ok(None) => {
                ::std::thread::sleep(::std::time::Duration::from_millis(10));
            }
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
            kill(Pid::from_raw(child.id() as i32), Some(Signal::SIGTERM)).unwrap();
            continue;
        } else {
            // when I say whoa, I mean WHOA!
            let _ = child.kill();
            break 'handle_exit;
        }
    }
}

fn handle_openpty(
    open_pty_res: OpenptyResult,
    cmd: RunCommand,
    quit_cb: Box<dyn Fn(PaneId) + Send>,
) -> (RawFd, RawFd) {
    // primary side of pty and child fd
    let pid_primary = open_pty_res.master;
    let pid_secondary = open_pty_res.slave;

    let mut child = unsafe {
        let command = &mut Command::new(cmd.command);
        if let Some(current_dir) = cmd.cwd {
            if current_dir.exists() {
                command.current_dir(current_dir);
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
        child.wait().unwrap();
        handle_command_exit(child);
        let _ = nix::unistd::close(pid_primary);
        let _ = nix::unistd::close(pid_secondary);
        quit_cb(PaneId::Terminal(pid_primary));
    });

    (pid_primary, child_id as RawFd)
}

/// Spawns a new terminal from the parent terminal with [`termios`](termios::Termios)
/// `orig_termios`.
///
fn handle_terminal(
    cmd: RunCommand,
    orig_termios: termios::Termios,
    quit_cb: Box<dyn Fn(PaneId) + Send>,
) -> (RawFd, RawFd) {
    // Create a pipe to allow the child the communicate the shell's pid to it's
    // parent.
    match openpty(None, Some(&orig_termios)) {
        Ok(open_pty_res) => handle_openpty(open_pty_res, cmd, quit_cb),
        Err(e) => {
            panic!("failed to start pty{:?}", e);
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
pub fn spawn_terminal(
    terminal_action: TerminalAction,
    orig_termios: termios::Termios,
    quit_cb: Box<dyn Fn(PaneId) + Send>,
) -> (RawFd, RawFd) {
    let cmd = match terminal_action {
        TerminalAction::OpenFile(file_to_open) => {
            if env::var("EDITOR").is_err() && env::var("VISUAL").is_err() {
                panic!("Can't edit files if an editor is not defined. To fix: define the EDITOR or VISUAL environment variables with the path to your editor (eg. /usr/bin/vim)");
            }
            let command =
                PathBuf::from(env::var("EDITOR").unwrap_or_else(|_| env::var("VISUAL").unwrap()));

            let args = vec![file_to_open
                .into_os_string()
                .into_string()
                .expect("Not valid Utf8 Encoding")];
            RunCommand {
                command,
                args,
                cwd: None,
            }
        }
        TerminalAction::RunCommand(command) => command,
    };

    handle_terminal(cmd, orig_termios, quit_cb)
}

#[derive(Clone)]
pub struct ServerOsInputOutput {
    orig_termios: Arc<Mutex<termios::Termios>>,
    client_senders: Arc<Mutex<HashMap<ClientId, IpcSenderWithContext<ServerToClientMsg>>>>,
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
    /// Sets the size of the terminal associated to file descriptor `fd`.
    fn set_terminal_size_using_fd(&self, fd: RawFd, cols: u16, rows: u16);
    /// Spawn a new terminal, with a terminal action. The returned tuple contains the master file
    /// descriptor of the forked psuedo terminal and a [ChildId] struct containing process id's for
    /// the forked child process.
    fn spawn_terminal(
        &self,
        terminal_action: TerminalAction,
        quit_cb: Box<dyn Fn(PaneId) + Send>,
    ) -> (RawFd, RawFd);
    /// Read bytes from the standard output of the virtual terminal referred to by `fd`.
    fn read_from_tty_stdout(&self, fd: RawFd, buf: &mut [u8]) -> Result<usize, nix::Error>;
    /// Creates an `AsyncReader` that can be used to read from `fd` in an async context
    fn async_file_reader(&self, fd: RawFd) -> Box<dyn AsyncReader>;
    /// Write bytes to the standard input of the virtual terminal referred to by `fd`.
    fn write_to_tty_stdin(&self, fd: RawFd, buf: &[u8]) -> Result<usize, nix::Error>;
    /// Wait until all output written to the object referred to by `fd` has been transmitted.
    fn tcdrain(&self, fd: RawFd) -> Result<(), nix::Error>;
    /// Terminate the process with process ID `pid`. (SIGTERM)
    fn kill(&self, pid: Pid) -> Result<(), nix::Error>;
    /// Terminate the process with process ID `pid`. (SIGKILL)
    fn force_kill(&self, pid: Pid) -> Result<(), nix::Error>;
    /// Returns a [`Box`] pointer to this [`ServerOsApi`] struct.
    fn box_clone(&self) -> Box<dyn ServerOsApi>;
    fn send_to_client(&self, client_id: ClientId, msg: ServerToClientMsg);
    fn new_client(
        &mut self,
        client_id: ClientId,
        stream: LocalSocketStream,
    ) -> IpcReceiverWithContext<ClientToServerMsg>;
    fn remove_client(&mut self, client_id: ClientId);
    fn load_palette(&self) -> Palette;
    /// Returns the current working directory for a given pid
    fn get_cwd(&self, pid: Pid) -> Option<PathBuf>;
}

impl ServerOsApi for ServerOsInputOutput {
    fn set_terminal_size_using_fd(&self, fd: RawFd, cols: u16, rows: u16) {
        if cols > 0 && rows > 0 {
            set_terminal_size_using_fd(fd, cols, rows);
        }
    }
    fn spawn_terminal(
        &self,
        terminal_action: TerminalAction,
        quit_cb: Box<dyn Fn(PaneId) + Send>,
    ) -> (RawFd, RawFd) {
        let orig_termios = self.orig_termios.lock().unwrap();
        spawn_terminal(terminal_action, orig_termios.clone(), quit_cb)
    }
    fn read_from_tty_stdout(&self, fd: RawFd, buf: &mut [u8]) -> Result<usize, nix::Error> {
        unistd::read(fd, buf)
    }
    fn async_file_reader(&self, fd: RawFd) -> Box<dyn AsyncReader> {
        Box::new(RawFdAsyncReader::new(fd))
    }
    fn write_to_tty_stdin(&self, fd: RawFd, buf: &[u8]) -> Result<usize, nix::Error> {
        unistd::write(fd, buf)
    }
    fn tcdrain(&self, fd: RawFd) -> Result<(), nix::Error> {
        termios::tcdrain(fd)
    }
    fn box_clone(&self) -> Box<dyn ServerOsApi> {
        Box::new((*self).clone())
    }
    fn kill(&self, pid: Pid) -> Result<(), nix::Error> {
        let _ = kill(pid, Some(Signal::SIGHUP));
        Ok(())
    }
    fn force_kill(&self, pid: Pid) -> Result<(), nix::Error> {
        let _ = kill(pid, Some(Signal::SIGKILL));
        Ok(())
    }
    fn send_to_client(&self, client_id: ClientId, msg: ServerToClientMsg) {
        if let Some(sender) = self.client_senders.lock().unwrap().get_mut(&client_id) {
            sender.send(msg);
        }
    }
    fn new_client(
        &mut self,
        client_id: ClientId,
        stream: LocalSocketStream,
    ) -> IpcReceiverWithContext<ClientToServerMsg> {
        let receiver = IpcReceiverWithContext::new(stream);
        let sender = receiver.get_sender();
        self.client_senders
            .lock()
            .unwrap()
            .insert(client_id, sender);
        receiver
    }
    fn remove_client(&mut self, client_id: ClientId) {
        let mut client_senders = self.client_senders.lock().unwrap();
        if client_senders.contains_key(&client_id) {
            client_senders.remove(&client_id);
        }
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

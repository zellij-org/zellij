use crate::panes::PositionAndSize;
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use nix::pty::{forkpty, Winsize};
use nix::sys::signal::{kill, Signal};
use nix::sys::termios;
use nix::sys::wait::waitpid;
use nix::unistd;
use nix::unistd::{ForkResult, Pid};
use std::io;
use std::io::prelude::*;
use std::os::unix::io::RawFd;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};

use std::env;

fn into_raw_mode(pid: RawFd) {
    let mut tio = termios::tcgetattr(pid).expect("could not get terminal attribute");
    termios::cfmakeraw(&mut tio);
    match termios::tcsetattr(pid, termios::SetArg::TCSANOW, &tio) {
        Ok(_) => {}
        Err(e) => panic!("error {:?}", e),
    };
}

fn unset_raw_mode(pid: RawFd, orig_termios: termios::Termios) {
    match termios::tcsetattr(pid, termios::SetArg::TCSANOW, &orig_termios) {
        Ok(_) => {}
        Err(e) => panic!("error {:?}", e),
    };
}

pub fn get_terminal_size_using_fd(fd: RawFd) -> PositionAndSize {
    // TODO: do this with the nix ioctl
    use libc::ioctl;
    use libc::TIOCGWINSZ;

    let mut winsize = Winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    unsafe { ioctl(fd, TIOCGWINSZ, &mut winsize) };
    PositionAndSize::from(winsize)
}

pub fn set_terminal_size_using_fd(fd: RawFd, columns: u16, rows: u16) {
    // TODO: do this with the nix ioctl
    use libc::ioctl;
    use libc::TIOCSWINSZ;

    let winsize = Winsize {
        ws_col: columns,
        ws_row: rows,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    unsafe { ioctl(fd, TIOCSWINSZ, &winsize) };
}

/// Handle some signals for the child process. This will loop until the child
/// process exits.
fn handle_command_exit(mut child: Child) {
    use signal_hook::{consts::signal::SIGINT, iterator::Signals};
    use std::{thread::sleep, time::Duration};

    // register the SIGINT signal (TODO handle more signals)
    let mut signals = Signals::new(&[SIGINT]).unwrap();
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
                sleep(Duration::from_millis(100));
            }
            Err(e) => panic!("error attempting to wait: {}", e),
        }

        for signal in signals.pending() {
            // FIXME: We need to handle more signals here!
            match signal {
                SIGINT => {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    break 'handle_exit;
                }
                _ => {}
            }
        }
    }
}

/// Spawns a new terminal from the parent terminal with [`termios`](termios::Termios)
/// `orig_termios`.
///
/// If a `file_to_open` is given, the text editor specified by environment variable `EDITOR`
/// (or `VISUAL`, if `EDITOR` is not set) will be started in the new terminal, with the given
/// file open. If no file is given, the shell specified by environment variable `SHELL` will
/// be started in the new terminal.
///
/// # Panics
///
/// This function will panic if both the `EDITOR` and `VISUAL` environment variables are not
/// set.
// FIXME this should probably be split into different functions, or at least have less levels
// of indentation in some way
fn spawn_terminal(file_to_open: Option<PathBuf>, orig_termios: termios::Termios) -> (RawFd, RawFd) {
    let (pid_primary, pid_secondary): (RawFd, RawFd) = {
        match forkpty(None, Some(&orig_termios)) {
            Ok(fork_pty_res) => {
                let pid_primary = fork_pty_res.master;
                let pid_secondary = match fork_pty_res.fork_result {
                    ForkResult::Parent { child } => {
                        // fcntl(pid_primary, FcntlArg::F_SETFL(OFlag::empty())).expect("could not fcntl");
                        fcntl(pid_primary, FcntlArg::F_SETFL(OFlag::O_NONBLOCK))
                            .expect("could not fcntl");
                        child
                    }
                    ForkResult::Child => match file_to_open {
                        Some(file_to_open) => {
                            if env::var("EDITOR").is_err() && env::var("VISUAL").is_err() {
                                panic!("Can't edit files if an editor is not defined. To fix: define the EDITOR or VISUAL environment variables with the path to your editor (eg. /usr/bin/vim)");
                            }
                            let editor =
                                env::var("EDITOR").unwrap_or_else(|_| env::var("VISUAL").unwrap());

                            let child = Command::new(editor)
                                .args(&[file_to_open])
                                .spawn()
                                .expect("failed to spawn");
                            handle_command_exit(child);
                            ::std::process::exit(0);
                        }
                        None => {
                            let child = Command::new(env::var("SHELL").unwrap())
                                .spawn()
                                .expect("failed to spawn");
                            handle_command_exit(child);
                            ::std::process::exit(0);
                        }
                    },
                };
                (pid_primary, pid_secondary.as_raw())
            }
            Err(e) => {
                panic!("failed to fork {:?}", e);
            }
        }
    };
    (pid_primary, pid_secondary)
}

#[derive(Clone)]
pub struct OsInputOutput {
    orig_termios: Arc<Mutex<termios::Termios>>,
}

/// The `OsApi` trait represents an abstract interface to the features of an operating system that
/// Zellij requires.
pub trait OsApi: Send + Sync {
    /// Returns the size of the terminal associated to file descriptor `fd`.
    fn get_terminal_size_using_fd(&self, fd: RawFd) -> PositionAndSize;
    /// Sets the size of the terminal associated to file descriptor `fd`.
    fn set_terminal_size_using_fd(&mut self, fd: RawFd, cols: u16, rows: u16);
    /// Set the terminal associated to file descriptor `fd` to
    /// [raw mode](https://en.wikipedia.org/wiki/Terminal_mode).
    fn set_raw_mode(&mut self, fd: RawFd);
    /// Set the terminal associated to file descriptor `fd` to
    /// [cooked mode](https://en.wikipedia.org/wiki/Terminal_mode).
    fn unset_raw_mode(&mut self, fd: RawFd);
    /// Spawn a new terminal, with an optional file to open in a terminal program.
    fn spawn_terminal(&mut self, file_to_open: Option<PathBuf>) -> (RawFd, RawFd);
    /// Read bytes from the standard output of the virtual terminal referred to by `fd`.
    fn read_from_tty_stdout(&mut self, fd: RawFd, buf: &mut [u8]) -> Result<usize, nix::Error>;
    /// Write bytes to the standard input of the virtual terminal referred to by `fd`.
    fn write_to_tty_stdin(&mut self, fd: RawFd, buf: &mut [u8]) -> Result<usize, nix::Error>;
    /// Wait until all output written to the object referred to by `fd` has been transmitted.
    fn tcdrain(&mut self, fd: RawFd) -> Result<(), nix::Error>;
    /// Terminate the process with process ID `pid`.
    // FIXME `RawFd` is semantically the wrong type here. It should either be a raw libc::pid_t,
    // or a nix::unistd::Pid. See `man kill.3`, nix::sys::signal::kill (both take an argument
    // called `pid` and of type `pid_t`, and not `fd`)
    fn kill(&mut self, pid: RawFd) -> Result<(), nix::Error>;
    /// Returns the raw contents of standard input.
    fn read_from_stdin(&self) -> Vec<u8>;
    /// Returns the writer that allows writing to standard output.
    fn get_stdout_writer(&self) -> Box<dyn io::Write>;
    /// Returns a [`Box`] pointer to this [`OsApi`] struct.
    fn box_clone(&self) -> Box<dyn OsApi>;
}

impl OsApi for OsInputOutput {
    fn get_terminal_size_using_fd(&self, fd: RawFd) -> PositionAndSize {
        get_terminal_size_using_fd(fd)
    }
    fn set_terminal_size_using_fd(&mut self, fd: RawFd, cols: u16, rows: u16) {
        set_terminal_size_using_fd(fd, cols, rows);
    }
    fn set_raw_mode(&mut self, fd: RawFd) {
        into_raw_mode(fd);
    }
    fn unset_raw_mode(&mut self, fd: RawFd) {
        let orig_termios = self.orig_termios.lock().unwrap();
        unset_raw_mode(fd, orig_termios.clone());
    }
    fn spawn_terminal(&mut self, file_to_open: Option<PathBuf>) -> (RawFd, RawFd) {
        let orig_termios = self.orig_termios.lock().unwrap();
        spawn_terminal(file_to_open, orig_termios.clone())
    }
    fn read_from_tty_stdout(&mut self, fd: RawFd, buf: &mut [u8]) -> Result<usize, nix::Error> {
        unistd::read(fd, buf)
    }
    fn write_to_tty_stdin(&mut self, fd: RawFd, buf: &mut [u8]) -> Result<usize, nix::Error> {
        unistd::write(fd, buf)
    }
    fn tcdrain(&mut self, fd: RawFd) -> Result<(), nix::Error> {
        termios::tcdrain(fd)
    }
    fn box_clone(&self) -> Box<dyn OsApi> {
        Box::new((*self).clone())
    }
    fn read_from_stdin(&self) -> Vec<u8> {
        let stdin = std::io::stdin();
        let mut stdin = stdin.lock();
        let buffer = stdin.fill_buf().unwrap();
        let length = buffer.len();
        let read_bytes = Vec::from(buffer);
        stdin.consume(length);
        read_bytes
    }
    fn get_stdout_writer(&self) -> Box<dyn io::Write> {
        let stdout = ::std::io::stdout();
        Box::new(stdout)
    }
    fn kill(&mut self, pid: RawFd) -> Result<(), nix::Error> {
        kill(Pid::from_raw(pid), Some(Signal::SIGINT)).unwrap();
        waitpid(Pid::from_raw(pid), None).unwrap();
        Ok(())
    }
}

impl Clone for Box<dyn OsApi> {
    fn clone(&self) -> Box<dyn OsApi> {
        self.box_clone()
    }
}

pub fn get_os_input() -> OsInputOutput {
    let current_termios = termios::tcgetattr(0).unwrap();
    let orig_termios = Arc::new(Mutex::new(current_termios));
    OsInputOutput { orig_termios }
}

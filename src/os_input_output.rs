use crate::panes::PositionAndSize;
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use nix::pty::{forkpty, Winsize};
use nix::sys::signal::{kill, Signal};
use nix::sys::termios::{cfmakeraw, tcdrain, tcgetattr, tcsetattr, SetArg, Termios};
use nix::sys::wait::waitpid;
use nix::unistd::{read, write, ForkResult, Pid};
use std::io::prelude::*;
use std::io::{stdin, Write};
use std::os::unix::io::RawFd;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};

use std::env;

fn into_raw_mode(pid: RawFd) {
    let mut tio = tcgetattr(pid).expect("could not get terminal attribute");
    cfmakeraw(&mut tio);
    match tcsetattr(pid, SetArg::TCSANOW, &tio) {
        Ok(_) => {}
        Err(e) => panic!("error {:?}", e),
    };
}

fn unset_raw_mode(pid: RawFd, orig_termios: Termios) {
    match tcsetattr(pid, SetArg::TCSANOW, &orig_termios) {
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

fn handle_command_exit(mut child: Child) {
    use signal_hook::consts::signal::SIGINT;

    let mut signals = signal_hook::iterator::Signals::new(&[SIGINT]).unwrap();
    'handle_exit: loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                // TODO: handle errors?
                break;
            }
            Ok(None) => {
                ::std::thread::sleep(::std::time::Duration::from_millis(100));
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

fn spawn_terminal(file_to_open: Option<PathBuf>, orig_termios: Termios) -> (RawFd, RawFd) {
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
    orig_termios: Arc<Mutex<Termios>>,
}

pub trait OsApi: Send + Sync {
    fn get_terminal_size_using_fd(&self, pid: RawFd) -> PositionAndSize;
    fn set_terminal_size_using_fd(&mut self, pid: RawFd, cols: u16, rows: u16);
    fn set_raw_mode(&mut self, pid: RawFd);
    fn unset_raw_mode(&mut self, pid: RawFd);
    fn spawn_terminal(&mut self, file_to_open: Option<PathBuf>) -> (RawFd, RawFd);
    fn read_from_tty_stdout(&mut self, pid: RawFd, buf: &mut [u8]) -> Result<usize, nix::Error>;
    fn write_to_tty_stdin(&mut self, pid: RawFd, buf: &mut [u8]) -> Result<usize, nix::Error>;
    fn tcdrain(&mut self, pid: RawFd) -> Result<(), nix::Error>;
    fn kill(&mut self, pid: RawFd) -> Result<(), nix::Error>;
    fn read_from_stdin(&self) -> Vec<u8>;
    fn get_stdout_writer(&self) -> Box<dyn Write>;
    fn box_clone(&self) -> Box<dyn OsApi>;
}

impl OsApi for OsInputOutput {
    fn get_terminal_size_using_fd(&self, pid: RawFd) -> PositionAndSize {
        get_terminal_size_using_fd(pid)
    }
    fn set_terminal_size_using_fd(&mut self, pid: RawFd, cols: u16, rows: u16) {
        set_terminal_size_using_fd(pid, cols, rows);
    }
    fn set_raw_mode(&mut self, pid: RawFd) {
        into_raw_mode(pid);
    }
    fn unset_raw_mode(&mut self, pid: RawFd) {
        let orig_termios = self.orig_termios.lock().unwrap();
        unset_raw_mode(pid, orig_termios.clone());
    }
    fn spawn_terminal(&mut self, file_to_open: Option<PathBuf>) -> (RawFd, RawFd) {
        let orig_termios = self.orig_termios.lock().unwrap();
        spawn_terminal(file_to_open, orig_termios.clone())
    }
    fn read_from_tty_stdout(&mut self, pid: RawFd, buf: &mut [u8]) -> Result<usize, nix::Error> {
        read(pid, buf)
    }
    fn write_to_tty_stdin(&mut self, pid: RawFd, buf: &mut [u8]) -> Result<usize, nix::Error> {
        write(pid, buf)
    }
    fn tcdrain(&mut self, pid: RawFd) -> Result<(), nix::Error> {
        tcdrain(pid)
    }
    fn box_clone(&self) -> Box<dyn OsApi> {
        Box::new((*self).clone())
    }
    fn read_from_stdin(&self) -> Vec<u8> {
        let stdin = stdin();
        let mut stdin = stdin.lock();
        let buffer = stdin.fill_buf().unwrap();
        let length = buffer.len();
        let read_bytes = Vec::from(buffer);
        stdin.consume(length);
        read_bytes
    }
    fn get_stdout_writer(&self) -> Box<dyn Write> {
        let stdout = ::std::io::stdout();
        Box::new(stdout)
    }
    fn kill(&mut self, fd: RawFd) -> Result<(), nix::Error> {
        kill(Pid::from_raw(fd), Some(Signal::SIGINT)).unwrap();
        waitpid(Pid::from_raw(fd), None).unwrap();
        Ok(())
    }
}

impl Clone for Box<dyn OsApi> {
    fn clone(&self) -> Box<dyn OsApi> {
        self.box_clone()
    }
}

pub fn get_os_input() -> OsInputOutput {
    let current_termios = tcgetattr(0).unwrap();
    let orig_termios = Arc::new(Mutex::new(current_termios));
    OsInputOutput { orig_termios }
}

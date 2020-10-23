use nix::unistd::{read, write, ForkResult, Pid};
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use nix::sys::termios::{
    tcgetattr,
    cfmakeraw,
    tcsetattr,
    SetArg,
    tcdrain,
};
use nix::sys::wait::waitpid;
use nix::sys::signal::{kill, Signal};
use nix::pty::{forkpty, Winsize};
use std::os::unix::io::RawFd;
use std::process::{Command, Child};
use std::io::{Read, Write};
use std::path::PathBuf;

use std::env;

fn into_raw_mode(pid: RawFd) {
    let mut tio = tcgetattr(pid).expect("could not get terminal attribute");
    cfmakeraw(&mut tio);
    match tcsetattr(pid, SetArg::TCSANOW, &mut tio) {
        Ok(_) => {},
        Err(e) => panic!("error {:?}", e)
    };

}

pub fn get_terminal_size_using_fd(fd: RawFd) -> Winsize {
    // TODO: do this with the nix ioctl
    use libc::ioctl;
    use libc::TIOCGWINSZ;

    let mut winsize = Winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    unsafe { ioctl(fd, TIOCGWINSZ.into(), &mut winsize) };
    winsize
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
    unsafe { ioctl(fd, TIOCSWINSZ.into(), &winsize) };
}

fn debug_log_to_file (message: String) {
    use std::fs::OpenOptions;
    use std::io::prelude::*;
    let mut file = OpenOptions::new().append(true).create(true).open("/tmp/mosaic-log.txt").unwrap();
    file.write_all(message.as_bytes()).unwrap();
    file.write_all("\n".as_bytes()).unwrap();
}

fn handle_command_exit(mut child: Child) {
    let signals = ::signal_hook::iterator::Signals::new(&[::signal_hook::SIGINT]).unwrap();
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
            match signal {
                signal_hook::SIGINT => {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    break 'handle_exit;
                }
                _ => {}
            }
        };
    }
}

fn spawn_terminal (file_to_open: Option<PathBuf>) -> (RawFd, RawFd) {
    let (pid_primary, pid_secondary): (RawFd, RawFd) = {
        match forkpty(None, None) {
            Ok(fork_pty_res) => {
                let pid_primary = fork_pty_res.master;
                let pid_secondary = match fork_pty_res.fork_result {
                    ForkResult::Parent { child } => {
                        // fcntl(pid_primary, FcntlArg::F_SETFL(OFlag::empty())).expect("could not fcntl");
                        fcntl(pid_primary, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)).expect("could not fcntl");
                        child
                    },
                    ForkResult::Child => {
                        match file_to_open {
                            Some(file_to_open) => {
                                if env::var("EDITOR").is_err() && env::var("VISUAL").is_err() {
                                    panic!("Can't edit files if an editor is not defined. To fix: define the EDITOR or VISUAL environment variables with the path to your editor (eg. /usr/bin/vim)");
                                }
                                let editor = env::var("EDITOR").unwrap_or_else(|_| env::var("VISUAL").unwrap());

                                let child = Command::new(editor).args(&[file_to_open]).spawn().expect("failed to spawn");
                                handle_command_exit(child);
                                ::std::process::exit(0);
                            },
                            None => {
                                let child = Command::new(env::var("SHELL").unwrap()).spawn().expect("failed to spawn");
                                handle_command_exit(child);
                                ::std::process::exit(0);
                            }
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
pub struct OsInputOutput {}

pub trait OsApi: Send + Sync {
    fn get_terminal_size_using_fd(&self, pid: RawFd) -> Winsize;
    fn set_terminal_size_using_fd(&mut self, pid: RawFd, cols: u16, rows: u16);
    fn into_raw_mode(&mut self, pid: RawFd);
    fn spawn_terminal(&mut self, file_to_open: Option<PathBuf>) -> (RawFd, RawFd);
    fn read_from_tty_stdout(&mut self, pid: RawFd, buf: &mut [u8]) -> Result<usize, nix::Error>;
    fn write_to_tty_stdin(&mut self, pid: RawFd, buf: &mut [u8]) -> Result<usize, nix::Error>;
    fn tcdrain(&mut self, pid: RawFd) -> Result<(), nix::Error>;
    fn kill(&mut self, pid: RawFd) -> Result<(), nix::Error>;
    fn get_stdin_reader(&self) -> Box<dyn Read>;
    fn get_stdout_writer(&self) -> Box<dyn Write>;
    fn box_clone(&self) -> Box<dyn OsApi>;
}

impl OsApi for OsInputOutput {
    fn get_terminal_size_using_fd(&self, pid: RawFd) -> Winsize {
        get_terminal_size_using_fd(pid)
    }
    fn set_terminal_size_using_fd(&mut self, pid: RawFd, cols: u16, rows: u16) {
        set_terminal_size_using_fd(pid, cols, rows);
    }
    fn into_raw_mode(&mut self, pid: RawFd) {
        into_raw_mode(pid);
    }
    fn spawn_terminal(&mut self, file_to_open: Option<PathBuf>) -> (RawFd, RawFd) {
        spawn_terminal(file_to_open)
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
    fn get_stdin_reader(&self) -> Box<dyn Read> {
        // TODO: stdin lock, right now it's not done because we don't have where to put it
        // if we put it on the struct, we won't be able to clone the struct
        // if we leave it here, we're referencing a temporary value
        let stdin = ::std::io::stdin();
        Box::new(stdin)
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

impl Clone for Box<dyn OsApi>
{
    fn clone(&self) -> Box<dyn OsApi> {
        self.box_clone()
    }
}

pub fn get_os_input () -> OsInputOutput {
    OsInputOutput {}
}

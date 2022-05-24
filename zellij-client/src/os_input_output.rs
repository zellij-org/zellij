use zellij_utils::pane_size::Size;
use zellij_utils::{interprocess, libc, nix, signal_hook, zellij_tile};

use interprocess::local_socket::LocalSocketStream;
use mio::{unix::SourceFd, Events, Interest, Poll, Token};
use nix::pty::Winsize;
use nix::sys::termios;
use signal_hook::{consts::signal::*, iterator::Signals};
use std::io::prelude::*;
use std::os::unix::io::RawFd;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::{io, thread, time};
use zellij_tile::data::Palette;
use zellij_utils::{
    errors::ErrorContext,
    ipc::{ClientToServerMsg, IpcReceiverWithContext, IpcSenderWithContext, ServerToClientMsg},
    shared::default_palette,
};

const SIGWINCH_CB_THROTTLE_DURATION: time::Duration = time::Duration::from_millis(50);

const ENABLE_MOUSE_SUPPORT: &str = "\u{1b}[?1000h\u{1b}[?1002h\u{1b}[?1015h\u{1b}[?1006h";
const DISABLE_MOUSE_SUPPORT: &str = "\u{1b}[?1006l\u{1b}[?1015l\u{1b}[?1002l\u{1b}[?1000l";

fn into_raw_mode(pid: RawFd) {
    let mut tio = termios::tcgetattr(pid).expect("could not get terminal attribute");
    termios::cfmakeraw(&mut tio);
    match termios::tcsetattr(pid, termios::SetArg::TCSANOW, &tio) {
        Ok(_) => {}
        Err(e) => panic!("error {:?}", e),
    };
}

fn unset_raw_mode(pid: RawFd, orig_termios: termios::Termios) -> Result<(), nix::Error> {
    termios::tcsetattr(pid, termios::SetArg::TCSANOW, &orig_termios)
}

pub(crate) fn get_terminal_size_using_fd(fd: RawFd) -> Size {
    // TODO: do this with the nix ioctl
    use libc::ioctl;
    use libc::TIOCGWINSZ;

    let mut winsize = Winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    // TIOCGWINSZ is an u32, but the second argument to ioctl is u64 on
    // some platforms. When checked on Linux, clippy will complain about
    // useless conversion.
    #[allow(clippy::useless_conversion)]
    unsafe {
        ioctl(fd, TIOCGWINSZ.into(), &mut winsize)
    };
    Size {
        rows: winsize.ws_row as usize,
        cols: winsize.ws_col as usize,
    }
}

#[derive(Clone)]
pub struct ClientOsInputOutput {
    orig_termios: Arc<Mutex<termios::Termios>>,
    send_instructions_to_server: Arc<Mutex<Option<IpcSenderWithContext<ClientToServerMsg>>>>,
    receive_instructions_from_server: Arc<Mutex<Option<IpcReceiverWithContext<ServerToClientMsg>>>>,
}

/// The `ClientOsApi` trait represents an abstract interface to the features of an operating system that
/// Zellij client requires.
pub trait ClientOsApi: Send + Sync {
    /// Returns the size of the terminal associated to file descriptor `fd`.
    fn get_terminal_size_using_fd(&self, fd: RawFd) -> Size;
    /// Set the terminal associated to file descriptor `fd` to
    /// [raw mode](https://en.wikipedia.org/wiki/Terminal_mode).
    fn set_raw_mode(&mut self, fd: RawFd);
    /// Set the terminal associated to file descriptor `fd` to
    /// [cooked mode](https://en.wikipedia.org/wiki/Terminal_mode).
    fn unset_raw_mode(&self, fd: RawFd) -> Result<(), nix::Error>;
    /// Returns the writer that allows writing to standard output.
    fn get_stdout_writer(&self) -> Box<dyn io::Write>;
    fn get_stdin_reader(&self) -> Box<dyn io::Read>;
    /// Returns the raw contents of standard input.
    fn read_from_stdin(&self) -> Vec<u8>;
    /// Returns a [`Box`] pointer to this [`ClientOsApi`] struct.
    fn box_clone(&self) -> Box<dyn ClientOsApi>;
    /// Sends a message to the server.
    fn send_to_server(&self, msg: ClientToServerMsg);
    /// Receives a message on client-side IPC channel
    // This should be called from the client-side router thread only.
    fn recv_from_server(&self) -> Option<(ServerToClientMsg, ErrorContext)>;
    fn handle_signals(&self, sigwinch_cb: Box<dyn Fn()>, quit_cb: Box<dyn Fn()>);
    /// Establish a connection with the server socket.
    fn connect_to_server(&self, path: &Path);
    fn load_palette(&self) -> Palette;
    fn enable_mouse(&self);
    fn disable_mouse(&self);
    // Repeatedly send action, until stdin is readable again
    fn stdin_poller(&self) -> StdinPoller;
}

impl ClientOsApi for ClientOsInputOutput {
    fn get_terminal_size_using_fd(&self, fd: RawFd) -> Size {
        get_terminal_size_using_fd(fd)
    }
    fn set_raw_mode(&mut self, fd: RawFd) {
        into_raw_mode(fd);
    }
    fn unset_raw_mode(&self, fd: RawFd) -> Result<(), nix::Error> {
        let orig_termios = self.orig_termios.lock().unwrap();
        unset_raw_mode(fd, orig_termios.clone())
    }
    fn box_clone(&self) -> Box<dyn ClientOsApi> {
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
    fn get_stdin_reader(&self) -> Box<dyn io::Read> {
        let stdin = ::std::io::stdin();
        Box::new(stdin)
    }

    fn send_to_server(&self, msg: ClientToServerMsg) {
        self.send_instructions_to_server
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .send(msg);
    }
    fn recv_from_server(&self) -> Option<(ServerToClientMsg, ErrorContext)> {
        self.receive_instructions_from_server
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .recv()
    }
    fn handle_signals(&self, sigwinch_cb: Box<dyn Fn()>, quit_cb: Box<dyn Fn()>) {
        let mut sigwinch_cb_timestamp = time::Instant::now();
        let mut signals = Signals::new(&[SIGWINCH, SIGTERM, SIGINT, SIGQUIT, SIGHUP]).unwrap();
        for signal in signals.forever() {
            match signal {
                SIGWINCH => {
                    // throttle sigwinch_cb calls, reduce excessive renders while resizing
                    if sigwinch_cb_timestamp.elapsed() < SIGWINCH_CB_THROTTLE_DURATION {
                        thread::sleep(SIGWINCH_CB_THROTTLE_DURATION);
                    }
                    sigwinch_cb_timestamp = time::Instant::now();
                    sigwinch_cb();
                }
                SIGTERM | SIGINT | SIGQUIT | SIGHUP => {
                    quit_cb();
                    break;
                }
                _ => unreachable!(),
            }
        }
    }
    fn connect_to_server(&self, path: &Path) {
        let socket;
        loop {
            match LocalSocketStream::connect(path) {
                Ok(sock) => {
                    socket = sock;
                    break;
                }
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
        let sender = IpcSenderWithContext::new(socket);
        let receiver = sender.get_receiver();
        *self.send_instructions_to_server.lock().unwrap() = Some(sender);
        *self.receive_instructions_from_server.lock().unwrap() = Some(receiver);
    }
    fn load_palette(&self) -> Palette {
        // this was removed because termbg doesn't release stdin in certain scenarios (we know of
        // windows terminal and FreeBSD): https://github.com/zellij-org/zellij/issues/538
        //
        // let palette = default_palette();
        // let timeout = std::time::Duration::from_millis(100);
        // if let Ok(rgb) = termbg::rgb(timeout) {
        //     palette.bg = PaletteColor::Rgb((rgb.r as u8, rgb.g as u8, rgb.b as u8));
        //     // TODO: also dynamically get all other colors from the user's terminal
        //     // this should be done in the same method (OSC ]11), but there might be other
        //     // considerations here, hence using the library
        // };
        default_palette()
    }
    fn enable_mouse(&self) {
        let _ = self
            .get_stdout_writer()
            .write(ENABLE_MOUSE_SUPPORT.as_bytes())
            .unwrap();
    }

    fn disable_mouse(&self) {
        let _ = self
            .get_stdout_writer()
            .write(DISABLE_MOUSE_SUPPORT.as_bytes())
            .unwrap();
    }

    fn stdin_poller(&self) -> StdinPoller {
        StdinPoller::default()
    }
}

impl Clone for Box<dyn ClientOsApi> {
    fn clone(&self) -> Box<dyn ClientOsApi> {
        self.box_clone()
    }
}

pub fn get_client_os_input() -> Result<ClientOsInputOutput, nix::Error> {
    let current_termios = termios::tcgetattr(0)?;
    let orig_termios = Arc::new(Mutex::new(current_termios));
    Ok(ClientOsInputOutput {
        orig_termios,
        send_instructions_to_server: Arc::new(Mutex::new(None)),
        receive_instructions_from_server: Arc::new(Mutex::new(None)),
    })
}

pub const DEFAULT_STDIN_POLL_TIMEOUT_MS: u64 = 10;

pub struct StdinPoller {
    poll: Poll,
    events: Events,
    timeout: time::Duration,
}

impl StdinPoller {
    // use mio poll to check if stdin is readable without blocking
    pub fn ready(&mut self) -> bool {
        self.poll
            .poll(&mut self.events, Some(self.timeout))
            .expect("could not poll stdin for readiness");
        for event in &self.events {
            if event.token() == Token(0) && event.is_readable() {
                return true;
            }
        }
        false
    }
}

impl Default for StdinPoller {
    fn default() -> Self {
        let stdin = 0;
        let mut stdin_fd = SourceFd(&stdin);
        let events = Events::with_capacity(128);
        let poll = Poll::new().unwrap();
        poll.registry()
            .register(&mut stdin_fd, Token(0), Interest::READABLE)
            .expect("could not create stdin poll");

        let timeout = time::Duration::from_millis(DEFAULT_STDIN_POLL_TIMEOUT_MS);

        Self {
            poll,
            events,
            timeout,
        }
    }
}

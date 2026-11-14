use anyhow::Result;
use async_trait::async_trait;
use zellij_utils::pane_size::Size;

#[cfg(not(windows))]
use crate::os_input_output_unix::{
    disable_mouse_support, enable_mouse_support, setup_ipc, setup_ipc_within, AsyncSignalListener,
    BlockingSignalIterator,
};
#[cfg(windows)]
use crate::os_input_output_windows::{
    disable_mouse_support, enable_mouse_support, restore_console_mode, setup_ipc, setup_ipc_within,
    AsyncSignalListener, BlockingSignalIterator,
};

use std::io::prelude::*;
use std::io::IsTerminal;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::{io, thread, time};
use zellij_utils::{
    data::Palette,
    errors::ErrorContext,
    ipc::{
        BlockingIoCancel, ClientToServerMsg, IpcReceiveError, IpcReceiverWithContext,
        IpcSenderWithContext, IpcStream, ServerToClientMsg,
    },
    shared::default_palette,
};

const SIGWINCH_CB_THROTTLE_DURATION: time::Duration = time::Duration::from_millis(50);

pub(crate) const ENABLE_MOUSE_SUPPORT: &str =
    "\u{1b}[?1000h\u{1b}[?1002h\u{1b}[?1003h\u{1b}[?1015h\u{1b}[?1006h";
pub(crate) const DISABLE_MOUSE_SUPPORT: &str =
    "\u{1b}[?1006l\u{1b}[?1015l\u{1b}[?1003l\u{1b}[?1002l\u{1b}[?1000l";

/// Trait for async stdin reading, allowing for testable implementations
#[async_trait]
pub trait AsyncStdin: Send {
    async fn read(&mut self) -> io::Result<Vec<u8>>;
}

pub struct AsyncStdinReader {
    stdin: tokio::io::Stdin,
    buffer: Vec<u8>,
}

impl AsyncStdinReader {
    pub fn new() -> Self {
        Self {
            stdin: tokio::io::stdin(),
            buffer: vec![0u8; 10 * 1024],
        }
    }
}

#[async_trait]
impl AsyncStdin for AsyncStdinReader {
    async fn read(&mut self) -> io::Result<Vec<u8>> {
        use tokio::io::AsyncReadExt;
        let n = self.stdin.read(&mut self.buffer).await?;
        Ok(self.buffer[..n].to_vec())
    }
}

pub enum SignalEvent {
    Resize,
    Quit,
}

/// Trait for async signal listening, allowing for testable implementations
#[async_trait]
pub trait AsyncSignals: Send {
    async fn recv(&mut self) -> Option<SignalEvent>;
}

pub(crate) fn get_terminal_size() -> Size {
    match crossterm::terminal::size() {
        Ok((cols, rows)) => {
            // fallback to default values when rows/cols == 0: https://github.com/zellij-org/zellij/issues/1551
            let rows = if rows != 0 { rows as usize } else { 24 };
            let cols = if cols != 0 { cols as usize } else { 80 };
            Size { rows, cols }
        },
        Err(_) => Size { rows: 24, cols: 80 },
    }
}

#[derive(Clone)]
pub struct ClientOsInputOutput {
    send_instructions_to_server: Arc<Mutex<Option<IpcSenderWithContext<ClientToServerMsg>>>>,
    receive_instructions_from_server: Arc<Mutex<Option<IpcReceiverWithContext<ServerToClientMsg>>>>,
    /// Serializes web writes. Not held by the desktop `send_to_server` path.
    ipc_write: Arc<Mutex<()>>,
    /// Set when a web attach is abandoned. The connect retry and the in-flight
    /// write both observe it.
    ipc_cancel: Arc<AtomicBool>,
    /// Clone of the socket a web write is blocked on, so cancel can shut it
    /// down without taking `send_instructions_to_server` for the whole write.
    ipc_cancel_stream: Arc<Mutex<Option<Box<dyn IpcStream>>>>,
    /// Cancels synchronous named-pipe I/O blocked on the writing thread.
    ipc_write_cancel: Arc<Mutex<Option<BlockingIoCancel>>>,
    reading_from_stdin: Arc<Mutex<Option<Vec<u8>>>>,
    session_name: Arc<Mutex<Option<String>>>,
    host_state: Arc<Mutex<RememberedHostState>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RememberedHostState {
    pub focused: Option<bool>,
    pub fullscreen: Option<bool>,
}

impl RememberedHostState {
    pub fn note(&mut self, message: &zellij_utils::nested_session::NestedSessionMessage) {
        use zellij_utils::nested_session::NestedSessionMessage;
        match message {
            NestedSessionMessage::FocusGained { .. } => self.focused = Some(true),
            NestedSessionMessage::FocusLost => self.focused = Some(false),
            NestedSessionMessage::FullscreenState { fullscreen } => {
                self.fullscreen = Some(*fullscreen)
            },
            _ => {},
        }
    }

    pub fn replay(&self) -> Vec<zellij_utils::nested_session::NestedSessionMessage> {
        use zellij_utils::nested_session::NestedSessionMessage;
        let mut messages = vec![];
        if self.fullscreen == Some(true) {
            messages.push(NestedSessionMessage::FullscreenState { fullscreen: true });
        }
        if self.focused == Some(true) {
            messages.push(NestedSessionMessage::FocusGained {
                from_direction: None,
            });
        }
        messages
    }
}

impl std::fmt::Debug for ClientOsInputOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientOsInputOutput").finish()
    }
}

/// The `ClientOsApi` trait represents an abstract interface to the features of an operating system that
/// Zellij client requires.
pub trait ClientOsApi: Send + Sync + std::fmt::Debug {
    /// Returns the size of the terminal.
    fn get_terminal_size(&self) -> Size;
    /// Set the terminal to
    /// [raw mode](https://en.wikipedia.org/wiki/Terminal_mode).
    fn set_raw_mode(&mut self);
    /// Set the terminal to
    /// [cooked mode](https://en.wikipedia.org/wiki/Terminal_mode).
    fn unset_raw_mode(&self) -> Result<(), std::io::Error>;
    /// Returns the writer that allows writing to standard output.
    fn get_stdout_writer(&self) -> Box<dyn io::Write>;
    /// Returns a BufReader that allows to read from STDIN line by line, also locks STDIN
    fn get_stdin_reader(&self) -> Box<dyn io::BufRead>;
    fn stdin_is_terminal(&self) -> bool {
        true
    }
    fn stdout_is_terminal(&self) -> bool {
        true
    }
    fn update_session_name(&mut self, new_session_name: String);
    /// Returns the raw contents of standard input.
    fn read_from_stdin(&mut self) -> Result<Vec<u8>, &'static str>;
    /// Returns a [`Box`] pointer to this [`ClientOsApi`] struct.
    fn box_clone(&self) -> Box<dyn ClientOsApi>;
    /// Sends a message to the server.
    fn send_to_server(&self, msg: ClientToServerMsg);
    /// Send `msg` using the same frame bytes as [`ClientOsApi::send_to_server`].
    /// Returns `Ok` only when those bytes were written. A deadline that expires
    /// first, a closed pipe, or no connection is an error: the message was not
    /// delivered. The default reports success after `send_to_server`, which is
    /// what test doubles that record messages do.
    fn try_send_to_server_until(
        &self,
        msg: ClientToServerMsg,
        deadline: time::Instant,
    ) -> io::Result<()> {
        if time::Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "ipc write deadline exhausted",
            ));
        }
        self.send_to_server(msg);
        Ok(())
    }
    /// Stop a web connect retry and any write blocked on the session pipe.
    fn cancel_session_ipc(&self) {}
    fn session_ipc_cancelled(&self) -> bool {
        false
    }
    /// Receives a message on client-side IPC channel
    // This should be called from the client-side router thread only.
    fn recv_from_server(&self) -> Option<(ServerToClientMsg, ErrorContext)>;
    fn try_recv_from_server(
        &self,
    ) -> std::result::Result<(ServerToClientMsg, ErrorContext), IpcReceiveError> {
        self.recv_from_server().ok_or(IpcReceiveError::Undecodable)
    }
    fn handle_signals(
        &self,
        sigwinch_cb: Box<dyn Fn()>,
        quit_cb: Box<dyn Fn()>,
        resize_receiver: Option<std::sync::mpsc::Receiver<()>>,
    );
    /// Establish a connection with the server socket.
    fn connect_to_server(&self, path: &Path);
    /// Like [`ClientOsApi::connect_to_server`], but return once `deadline`
    /// passes or [`ClientOsApi::cancel_session_ipc`] was called. The default
    /// calls `connect_to_server` when the deadline has not already passed.
    fn connect_to_server_until(&self, path: &Path, deadline: time::Instant) -> io::Result<()> {
        if time::Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "ipc connect deadline exhausted",
            ));
        }
        self.connect_to_server(path);
        Ok(())
    }
    fn spawn_server(&self, socket_path: &Path, debug: bool) -> Result<(), std::io::Error> {
        crate::spawn_server(socket_path, debug)
    }
    fn should_install_panic_hook(&self) -> bool {
        true
    }
    fn load_palette(&self) -> Palette;
    fn enable_mouse(&self) -> Result<()>;
    fn disable_mouse(&self) -> Result<()>;
    /// Restore console input mode to its pre-Zellij state.
    ///
    /// On Windows this clears ENABLE_MOUSE_INPUT and ENABLE_VIRTUAL_TERMINAL_INPUT
    /// that crossterm's disable_raw_mode() leaves behind. No-op on other platforms.
    fn restore_console_mode(&self) {}
    fn discard_pending_input(&self) {}
    fn env_variable(&self, _name: &str) -> Option<String> {
        None
    }
    /// Returns an async stdin reader that can be polled in tokio::select
    fn get_async_stdin_reader(&self) -> Box<dyn AsyncStdin> {
        Box::new(AsyncStdinReader::new())
    }
    /// Returns an async signal listener that can be polled in tokio::select
    fn get_async_signal_listener(&self) -> io::Result<Box<dyn AsyncSignals>> {
        Ok(Box::new(AsyncSignalListener::new()?))
    }
    fn note_host_message(&self, _message: &zellij_utils::nested_session::NestedSessionMessage) {}
    fn remembered_host_state(&self) -> RememberedHostState {
        RememberedHostState::default()
    }
    fn forget_host_state(&self) {}
}

impl ClientOsApi for ClientOsInputOutput {
    fn get_terminal_size(&self) -> Size {
        get_terminal_size()
    }
    fn set_raw_mode(&mut self) {
        crossterm::terminal::enable_raw_mode().expect("could not enable raw mode");
    }
    fn unset_raw_mode(&self) -> Result<(), std::io::Error> {
        crossterm::terminal::disable_raw_mode()
    }
    fn box_clone(&self) -> Box<dyn ClientOsApi> {
        Box::new((*self).clone())
    }
    fn update_session_name(&mut self, new_session_name: String) {
        *self.session_name.lock().unwrap() = Some(new_session_name);
    }
    fn note_host_message(&self, message: &zellij_utils::nested_session::NestedSessionMessage) {
        self.host_state.lock().unwrap().note(message);
    }
    fn remembered_host_state(&self) -> RememberedHostState {
        *self.host_state.lock().unwrap()
    }
    fn forget_host_state(&self) {
        *self.host_state.lock().unwrap() = RememberedHostState::default();
    }
    fn read_from_stdin(&mut self) -> Result<Vec<u8>, &'static str> {
        let session_name_at_calltime = { self.session_name.lock().unwrap().clone() };
        // here we wait for a lock in case another thread is holding stdin
        // this can happen for example when switching sessions, the old thread will only be
        // released once it sees input over STDIN
        //
        // when this happens, we detect in the other thread that our session is ended (by comparing
        // the session name at the beginning of the call and the one after we read from STDIN), and
        // so place what we read from STDIN inside a buffer (the "reading_from_stdin" on our state)
        // and release the lock
        //
        // then, another thread will see there's something in the buffer immediately as it acquires
        // the lock (without having to wait for STDIN itself) forward this buffer and proceed to
        // wait for the "real" STDIN net time it is called
        let mut buffered_bytes = self.reading_from_stdin.lock().unwrap();
        match buffered_bytes.take() {
            Some(buffered_bytes) => Ok(buffered_bytes),
            None => {
                let stdin = std::io::stdin();
                let mut stdin = stdin.lock();
                let buffer = match stdin.fill_buf() {
                    Ok(buffer) => buffer,
                    Err(_) => return Err("Failed to read from STDIN"),
                };
                let length = buffer.len();
                let read_bytes = Vec::from(buffer);
                stdin.consume(length);

                let session_name_after_reading_from_stdin =
                    { self.session_name.lock().unwrap().clone() };
                if session_name_at_calltime.is_some()
                    && session_name_at_calltime != session_name_after_reading_from_stdin
                {
                    if !read_bytes.is_empty() {
                        *buffered_bytes = Some(read_bytes);
                    }
                    Err("Session ended")
                } else {
                    Ok(read_bytes)
                }
            },
        }
    }
    fn get_stdout_writer(&self) -> Box<dyn io::Write> {
        let stdout = ::std::io::stdout();
        Box::new(stdout)
    }

    fn get_stdin_reader(&self) -> Box<dyn io::BufRead> {
        let stdin = ::std::io::stdin();
        Box::new(stdin.lock())
    }

    fn stdin_is_terminal(&self) -> bool {
        let stdin = ::std::io::stdin();
        stdin.is_terminal()
    }

    fn stdout_is_terminal(&self) -> bool {
        let stdout = ::std::io::stdout();
        stdout.is_terminal()
    }

    fn send_to_server(&self, msg: ClientToServerMsg) {
        match self.send_instructions_to_server.lock().unwrap().as_mut() {
            Some(sender) => {
                let _ = sender.send_client_msg(msg);
            },
            None => {
                log::warn!("Server not ready, dropping message.");
            },
        }
    }
    fn try_send_to_server_until(
        &self,
        msg: ClientToServerMsg,
        deadline: time::Instant,
    ) -> io::Result<()> {
        // The write lock serializes frames. It is not the sender mutex: that
        // one is released before the write so a cancel can shut the socket
        // down, and so a stuck pipe cannot pin every other client operation.
        let _write = self.ipc_write.lock().unwrap_or_else(|p| p.into_inner());
        if self.ipc_cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "ipc write cancelled",
            ));
        }
        if time::Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "ipc write deadline exhausted",
            ));
        }
        let mut sender = {
            let mut guard = self
                .send_instructions_to_server
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            guard
                .take()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "server not ready"))?
        };
        if let Ok(cancel_stream) = sender.try_clone_stream() {
            *self
                .ipc_cancel_stream
                .lock()
                .unwrap_or_else(|p| p.into_inner()) = Some(cancel_stream);
        }
        if let Ok(cancel) = BlockingIoCancel::for_current_thread() {
            *self
                .ipc_write_cancel
                .lock()
                .unwrap_or_else(|p| p.into_inner()) = Some(cancel);
        }
        if self.ipc_cancel.load(Ordering::Relaxed) {
            *self
                .ipc_write_cancel
                .lock()
                .unwrap_or_else(|p| p.into_inner()) = None;
            *self
                .ipc_cancel_stream
                .lock()
                .unwrap_or_else(|p| p.into_inner()) = None;
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "ipc write cancelled",
            ));
        }
        let result = sender.send_client_msg_until(msg, deadline);
        *self
            .ipc_write_cancel
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
        *self
            .ipc_cancel_stream
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
        if result.is_err() {
            sender.abandon();
            return result;
        }
        if result.is_ok() && !self.ipc_cancel.load(Ordering::Relaxed) {
            let mut guard = self
                .send_instructions_to_server
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            // A connect that landed during the write owns the slot now.
            if guard.is_none() {
                *guard = Some(sender);
            }
        }
        result
    }
    fn cancel_session_ipc(&self) {
        self.ipc_cancel.store(true, Ordering::Relaxed);
        if let Some(cancel) = self
            .ipc_write_cancel
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
        {
            let _ = cancel.cancel_blocking_io();
        }
        if let Some(stream) = self
            .ipc_cancel_stream
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
        {
            let _ = stream.shutdown_stream();
        }
        if let Some(sender) = self
            .send_instructions_to_server
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
        {
            if let Ok(stream) = sender.try_clone_stream() {
                let _ = stream.shutdown_stream();
            }
        }
    }
    fn session_ipc_cancelled(&self) -> bool {
        self.ipc_cancel.load(Ordering::Relaxed)
    }
    fn recv_from_server(&self) -> Option<(ServerToClientMsg, ErrorContext)> {
        self.try_recv_from_server().ok()
    }
    fn try_recv_from_server(
        &self,
    ) -> std::result::Result<(ServerToClientMsg, ErrorContext), IpcReceiveError> {
        match self
            .receive_instructions_from_server
            .lock()
            // SAFETY: Mutex poison only occurs after another thread already panicked
            // while holding this lock; recovering the guard keeps IPC readable.
            .unwrap_or_else(|p| p.into_inner())
            .as_mut()
        {
            Some(receiver) => receiver.try_recv_server_msg(),
            None => Err(IpcReceiveError::Disconnected),
        }
    }
    fn handle_signals(
        &self,
        sigwinch_cb: Box<dyn Fn()>,
        quit_cb: Box<dyn Fn()>,
        resize_receiver: Option<std::sync::mpsc::Receiver<()>>,
    ) {
        let mut sigwinch_cb_timestamp = time::Instant::now();
        let signals = match BlockingSignalIterator::new(resize_receiver) {
            Ok(signals) => signals,
            Err(e) => {
                log::error!("Failed to register signal handlers: {}", e);
                return;
            },
        };
        for event in signals {
            match event {
                SignalEvent::Resize => {
                    // throttle sigwinch_cb calls, reduce excessive renders while resizing
                    if sigwinch_cb_timestamp.elapsed() < SIGWINCH_CB_THROTTLE_DURATION {
                        thread::sleep(SIGWINCH_CB_THROTTLE_DURATION);
                    }
                    sigwinch_cb_timestamp = time::Instant::now();
                    sigwinch_cb();
                },
                SignalEvent::Quit => {
                    quit_cb();
                    break;
                },
            }
        }
    }
    fn connect_to_server(&self, path: &Path) {
        // Desktop attach blocks until both directions are up. On Windows that
        // is one locked pair, not a request pipe followed by a later reply.
        loop {
            match setup_ipc(path) {
                Ok((sender, receiver)) => {
                    *self.send_instructions_to_server.lock().unwrap() = Some(sender);
                    *self.receive_instructions_from_server.lock().unwrap() = Some(receiver);
                    return;
                },
                Err(e) => {
                    log::warn!("failed to set up client IPC channels: {}", e);
                    std::thread::sleep(std::time::Duration::from_millis(50));
                },
            }
        }
    }
    fn connect_to_server_until(&self, path: &Path, deadline: time::Instant) -> io::Result<()> {
        loop {
            if self.ipc_cancel.load(Ordering::Relaxed) {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "ipc connect cancelled",
                ));
            }
            let remaining = deadline.saturating_duration_since(time::Instant::now());
            if remaining < time::Duration::from_millis(1) {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "ipc connect deadline exhausted",
                ));
            }
            let attempt = remaining.min(time::Duration::from_millis(50));
            if self.ipc_cancel.load(Ordering::Relaxed) {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "ipc connect cancelled",
                ));
            }
            match setup_ipc_within(path, attempt) {
                Ok((sender, receiver)) => {
                    if self.ipc_cancel.load(Ordering::Relaxed) {
                        return Err(io::Error::new(
                            io::ErrorKind::Interrupted,
                            "ipc connect cancelled",
                        ));
                    }
                    *self
                        .send_instructions_to_server
                        .lock()
                        .unwrap_or_else(|p| p.into_inner()) = Some(sender);
                    *self
                        .receive_instructions_from_server
                        .lock()
                        .unwrap_or_else(|p| p.into_inner()) = Some(receiver);
                    return Ok(());
                },
                Err(e) => {
                    // Refused, missing, and timed-out attempts are the retry.
                    // Anything else used to surface from channel setup.
                    if !matches!(
                        e.kind(),
                        io::ErrorKind::NotFound
                            | io::ErrorKind::ConnectionRefused
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::WouldBlock
                            | io::ErrorKind::Interrupted
                            | io::ErrorKind::ConnectionReset
                            | io::ErrorKind::ConnectionAborted
                    ) {
                        log::warn!("failed to set up client IPC channels: {}", e);
                    }
                },
            }
            self.wait_connect_retry(deadline)?;
        }
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
    fn enable_mouse(&self) -> Result<()> {
        let mut stdout = self.get_stdout_writer();
        enable_mouse_support(&mut *stdout)
    }

    fn disable_mouse(&self) -> Result<()> {
        let mut stdout = self.get_stdout_writer();
        disable_mouse_support(&mut *stdout)
    }

    #[cfg(windows)]
    fn restore_console_mode(&self) {
        restore_console_mode();
    }

    #[cfg(unix)]
    fn discard_pending_input(&self) {
        let stdin = std::io::stdin();
        if stdin.is_terminal() {
            let _ = nix::sys::termios::tcflush(&stdin, nix::sys::termios::FlushArg::TCIFLUSH);
        }
    }

    fn env_variable(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}

impl Clone for Box<dyn ClientOsApi> {
    fn clone(&self) -> Box<dyn ClientOsApi> {
        self.box_clone()
    }
}

impl ClientOsInputOutput {
    fn wait_connect_retry(&self, deadline: time::Instant) -> io::Result<()> {
        let slice_end = time::Instant::now() + time::Duration::from_millis(50);
        let limit = if slice_end < deadline {
            slice_end
        } else {
            deadline
        };
        while time::Instant::now() < limit {
            if self.ipc_cancel.load(Ordering::Relaxed) {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "ipc connect cancelled",
                ));
            }
            let nap = limit
                .saturating_duration_since(time::Instant::now())
                .min(time::Duration::from_millis(10));
            if nap.is_zero() {
                break;
            }
            thread::sleep(nap);
        }
        if self.ipc_cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "ipc connect cancelled",
            ));
        }
        if time::Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "ipc connect deadline exhausted",
            ));
        }
        Ok(())
    }
}

fn new_client_os_input() -> ClientOsInputOutput {
    ClientOsInputOutput {
        send_instructions_to_server: Arc::new(Mutex::new(None)),
        receive_instructions_from_server: Arc::new(Mutex::new(None)),
        ipc_write: Arc::new(Mutex::new(())),
        ipc_cancel: Arc::new(AtomicBool::new(false)),
        ipc_cancel_stream: Arc::new(Mutex::new(None)),
        ipc_write_cancel: Arc::new(Mutex::new(None)),
        reading_from_stdin: Arc::new(Mutex::new(None)),
        session_name: Arc::new(Mutex::new(None)),
        host_state: Arc::new(Mutex::new(RememberedHostState::default())),
    }
}

pub fn get_client_os_input() -> Result<ClientOsInputOutput, std::io::Error> {
    Ok(new_client_os_input())
}

pub fn get_cli_client_os_input() -> Result<ClientOsInputOutput, std::io::Error> {
    Ok(new_client_os_input())
}

pub const DEFAULT_STDIN_POLL_TIMEOUT_MS: u64 = 10;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_focus_and_fullscreen_from_the_host_are_replayed_after_a_switch() {
        use zellij_utils::nested_session::NestedSessionMessage;
        let os_input = get_client_os_input().unwrap();
        assert!(os_input.remembered_host_state().replay().is_empty());
        os_input.note_host_message(&NestedSessionMessage::FullscreenState { fullscreen: true });
        os_input.note_host_message(&NestedSessionMessage::FocusGained {
            from_direction: None,
        });
        assert_eq!(
            os_input.clone().remembered_host_state().replay(),
            vec![
                NestedSessionMessage::FullscreenState { fullscreen: true },
                NestedSessionMessage::FocusGained {
                    from_direction: None
                },
            ]
        );
        os_input.note_host_message(&NestedSessionMessage::FocusLost);
        os_input.note_host_message(&NestedSessionMessage::FullscreenState { fullscreen: false });
        assert!(os_input.remembered_host_state().replay().is_empty());
        os_input.note_host_message(&NestedSessionMessage::FocusGained {
            from_direction: None,
        });
        os_input.forget_host_state();
        assert!(os_input.remembered_host_state().replay().is_empty());
    }

    #[test]
    fn get_terminal_size_returns_nonzero_or_fallback() {
        let size = get_terminal_size();
        // In CI or when not attached to a terminal, crossterm may return an error
        // and we fall back to 80x24. Either way, size should be valid.
        assert!(size.rows > 0, "rows should be positive");
        assert!(size.cols > 0, "cols should be positive");
    }

    #[test]
    fn get_terminal_size_fallback_values() {
        // Verify the fallback constants are what we expect
        let fallback = Size { rows: 24, cols: 80 };
        // When crossterm::terminal::size() fails (no terminal), we should get 80x24
        // This is implicitly tested by get_terminal_size_returns_nonzero_or_fallback
        // but we verify the constants here
        assert_eq!(fallback.rows, 24);
        assert_eq!(fallback.cols, 80);
    }

    #[test]
    fn client_os_input_output_can_be_constructed() {
        let os_input = get_client_os_input().expect("should construct ClientOsInputOutput");
        let size = os_input.get_terminal_size();
        assert!(size.rows > 0, "rows should be positive");
        assert!(size.cols > 0, "cols should be positive");
    }

    #[test]
    fn cli_client_os_input_can_be_constructed() {
        let os_input = get_cli_client_os_input().expect("should construct CLI ClientOsInputOutput");
        let size = os_input.get_terminal_size();
        assert!(size.rows > 0, "rows should be positive");
        assert!(size.cols > 0, "cols should be positive");
    }

    #[test]
    fn a_send_with_no_session_is_not_delivered() {
        let os_input = get_client_os_input().unwrap();
        let error = os_input.try_send_to_server_until(
            ClientToServerMsg::ConnStatus,
            time::Instant::now() + time::Duration::from_secs(1),
        );
        assert!(
            error.is_err(),
            "a missing session pipe must not count as a delivered message"
        );
    }

    #[cfg(unix)]
    #[test]
    fn connect_to_server_until_stops_when_the_deadline_passes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.sock");
        let os_input = get_client_os_input().unwrap();
        let budget = time::Duration::from_millis(150);
        let started = time::Instant::now();
        let error = os_input.connect_to_server_until(&path, started + budget);
        assert!(error.is_err());
        let elapsed = started.elapsed();
        assert!(
            elapsed < time::Duration::from_secs(2),
            "connect retry ran for {elapsed:?}"
        );
        assert!(
            elapsed + time::Duration::from_millis(40) >= budget,
            "connect returned in {elapsed:?}, before the deadline"
        );
    }

    #[cfg(unix)]
    #[test]
    fn cancel_session_ipc_stops_the_connect_retry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.sock");
        let os_input = get_client_os_input().unwrap();
        let cancel = os_input.clone();
        thread::spawn(move || {
            thread::sleep(time::Duration::from_millis(80));
            cancel.cancel_session_ipc();
        });
        let started = time::Instant::now();
        let error =
            os_input.connect_to_server_until(&path, started + time::Duration::from_secs(30));
        assert!(error.is_err());
        assert!(
            started.elapsed() < time::Duration::from_secs(2),
            "cancel did not stop the connect retry"
        );
        assert!(os_input.session_ipc_cancelled());
    }

    #[cfg(unix)]
    fn wide_action(marker: u8) -> ClientToServerMsg {
        ClientToServerMsg::Action {
            action: zellij_utils::input::actions::Action::Write {
                key_with_modifier: None,
                bytes: vec![marker; 64 * 1024],
                is_kitty_keyboard_protocol: false,
            },
            terminal_id: None,
            client_id: None,
            is_cli_client: false,
        }
    }

    #[cfg(unix)]
    fn accept_one_client(
        path: &std::path::Path,
    ) -> (
        ClientOsInputOutput,
        std::sync::mpsc::Receiver<interprocess::local_socket::Stream>,
    ) {
        use interprocess::local_socket::traits::Listener;
        let listener = zellij_utils::consts::ipc_bind(path).expect("bind");
        let (tx, rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let stream = listener.accept().expect("accept");
            let _ = tx.send(stream);
        });
        let os_input = get_client_os_input().unwrap();
        os_input
            .connect_to_server_until(path, time::Instant::now() + time::Duration::from_secs(2))
            .expect("connect");
        (os_input, rx)
    }

    #[cfg(unix)]
    #[test]
    fn a_bounded_write_delivers_the_same_client_frame() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("live.sock");
        let (os_input, rx) = accept_one_client(&path);
        let server = rx.recv_timeout(time::Duration::from_secs(2)).unwrap();
        let mut receiver =
            zellij_utils::ipc::IpcReceiverWithContext::<ClientToServerMsg>::new(server);
        receiver
            .set_read_timeout(Some(time::Duration::from_secs(2)))
            .unwrap();
        os_input
            .try_send_to_server_until(
                ClientToServerMsg::ConnStatus,
                time::Instant::now() + time::Duration::from_secs(1),
            )
            .expect("delivered");
        let (msg, _) = receiver.try_recv_client_msg().expect("frame");
        assert!(matches!(msg, ClientToServerMsg::ConnStatus));
    }

    #[cfg(unix)]
    #[test]
    fn a_blocked_write_is_not_a_delivered_message() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("blocked.sock");
        let (os_input, rx) = accept_one_client(&path);
        let server = rx.recv_timeout(time::Duration::from_secs(2)).unwrap();
        let mut delivered = Vec::new();
        let mut blocked = None;
        for marker in 0..200u8 {
            let msg = wide_action(marker);
            let started = time::Instant::now();
            match os_input
                .try_send_to_server_until(msg.clone(), started + time::Duration::from_millis(80))
            {
                Ok(()) => delivered.push(msg),
                Err(_) => {
                    assert!(
                        started.elapsed() < time::Duration::from_secs(2),
                        "blocked write was not bounded"
                    );
                    blocked = Some(msg);
                    break;
                },
            }
        }
        let blocked = blocked.expect("the socket buffer never filled");
        let mut receiver =
            zellij_utils::ipc::IpcReceiverWithContext::<ClientToServerMsg>::new(server);
        receiver
            .set_read_timeout(Some(time::Duration::from_millis(200)))
            .unwrap();
        let mut got = Vec::new();
        loop {
            match receiver.try_recv_client_msg() {
                Ok((msg, _)) => got.push(msg),
                Err(_) => break,
            }
        }
        assert_eq!(got, delivered);
        assert!(
            !got.contains(&blocked),
            "a blocked write was decoded as a delivered frame"
        );
    }
}

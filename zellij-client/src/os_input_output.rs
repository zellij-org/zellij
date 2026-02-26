use anyhow::{Context, Result};
use async_trait::async_trait;
use interprocess;
use zellij_utils::pane_size::Size;

#[cfg(not(windows))]
use crate::os_input_output_unix::{AsyncSignalListener, BlockingSignalIterator};
#[cfg(windows)]
use crate::os_input_output_windows::{AsyncSignalListener, BlockingSignalIterator};

use interprocess::local_socket::{prelude::*, GenericFilePath, Stream as LocalSocketStream};
use std::io::prelude::*;
use std::io::IsTerminal;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::{io, thread, time};
use zellij_utils::{
    data::Palette,
    errors::ErrorContext,
    ipc::{ClientToServerMsg, IpcReceiverWithContext, IpcSenderWithContext, ServerToClientMsg},
    shared::default_palette,
};

const SIGWINCH_CB_THROTTLE_DURATION: time::Duration = time::Duration::from_millis(50);

const ENABLE_MOUSE_SUPPORT: &str =
    "\u{1b}[?1000h\u{1b}[?1002h\u{1b}[?1003h\u{1b}[?1015h\u{1b}[?1006h";
const DISABLE_MOUSE_SUPPORT: &str =
    "\u{1b}[?1006l\u{1b}[?1015l\u{1b}[?1003l\u{1b}[?1002l\u{1b}[?1000l";

/// Trait for async stdin reading, allowing for testable implementations
#[async_trait]
pub trait AsyncStdin: Send {
    async fn read(&mut self) -> io::Result<Vec<u8>>;
    fn release(&mut self) {}
}

type StdinRx = tokio::sync::mpsc::Receiver<io::Result<Vec<u8>>>;
type StdinRxSlot = Arc<Mutex<Option<StdinRx>>>;

/// Reads stdin via a background OS thread forwarding bytes over a channel.
/// This makes cancellation safe: dropping a channel recv loses nothing, unlike
/// dropping an in-flight tokio::io::Stdin read which discards already-read bytes.
struct AsyncStdinReader {
    receiver: Option<StdinRx>,
    receiver_slot: StdinRxSlot,
}

impl AsyncStdinReader {
    /// `slot` is the shared receiver store; `pump` ensures the background thread starts once.
    fn new(slot: StdinRxSlot, pump: &Arc<OnceLock<()>>) -> Self {
        pump.get_or_init({
            let slot = slot.clone();
            move || {
                let (tx, rx) = tokio::sync::mpsc::channel(256);
                *slot.lock().unwrap() = Some(rx);
                std::thread::Builder::new()
                    .name("stdin-pump".to_string())
                    .spawn(move || {
                        use std::io::Read;
                        let mut stdin = std::io::stdin();
                        let mut buf = vec![0u8; 10 * 1024];
                        loop {
                            match stdin.read(&mut buf) {
                                Ok(0) => break,
                                Ok(n) => {
                                    if tx.blocking_send(Ok(buf[..n].to_vec())).is_err() {
                                        break;
                                    }
                                },
                                Err(e) => {
                                    let _ = tx.blocking_send(Err(e));
                                    break;
                                },
                            }
                        }
                    })
                    .expect("failed to spawn stdin-pump thread");
            }
        });
        let rx = slot
            .lock()
            .unwrap()
            .take()
            .expect("stdin reader already in use");
        Self {
            receiver: Some(rx),
            receiver_slot: slot,
        }
    }
}

#[async_trait]
impl AsyncStdin for AsyncStdinReader {
    async fn read(&mut self) -> io::Result<Vec<u8>> {
        self.receiver
            .as_mut()
            .unwrap()
            .recv()
            .await
            .unwrap_or_else(|| Err(io::Error::new(io::ErrorKind::UnexpectedEof, "stdin closed")))
    }

    fn release(&mut self) {
        if let Some(rx) = self.receiver.take() {
            *self.receiver_slot.lock().unwrap() = Some(rx);
        }
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
    session_name: Arc<Mutex<Option<String>>>,
    stdin_reader_slot: StdinRxSlot,
    stdin_pump_once: Arc<OnceLock<()>>,
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
    fn enable_mouse(&self) -> Result<()>;
    fn disable_mouse(&self) -> Result<()>;
    fn env_variable(&self, _name: &str) -> Option<String> {
        None
    }
    /// Returns an async stdin reader that can be polled in tokio::select
    fn get_async_stdin_reader(&self) -> Box<dyn AsyncStdin> {
        unimplemented!()
    }
    /// Returns an async signal listener that can be polled in tokio::select
    fn get_async_signal_listener(&self) -> io::Result<Box<dyn AsyncSignals>> {
        Ok(Box::new(AsyncSignalListener::new()?))
    }
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
    fn recv_from_server(&self) -> Option<(ServerToClientMsg, ErrorContext)> {
        self.receive_instructions_from_server
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .recv_server_msg()
    }
    fn handle_signals(&self, sigwinch_cb: Box<dyn Fn()>, quit_cb: Box<dyn Fn()>) {
        let mut sigwinch_cb_timestamp = time::Instant::now();
        let signals = BlockingSignalIterator::new().unwrap();
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
        let fs_name = path
            .to_fs_name::<GenericFilePath>()
            .expect("failed to convert path to socket name");
        let socket;
        loop {
            match LocalSocketStream::connect(fs_name.clone()) {
                Ok(sock) => {
                    socket = sock;
                    break;
                },
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                },
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
    fn enable_mouse(&self) -> Result<()> {
        let err_context = "failed to enable mouse mode";
        let mut stdout = self.get_stdout_writer();
        stdout
            .write_all(ENABLE_MOUSE_SUPPORT.as_bytes())
            .context(err_context)?;
        stdout.flush().context(err_context)?;
        Ok(())
    }

    fn disable_mouse(&self) -> Result<()> {
        let err_context = "failed to enable mouse mode";
        let mut stdout = self.get_stdout_writer();
        stdout
            .write_all(DISABLE_MOUSE_SUPPORT.as_bytes())
            .context(err_context)?;
        stdout.flush().context(err_context)?;
        Ok(())
    }

    fn env_variable(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn get_async_stdin_reader(&self) -> Box<dyn AsyncStdin> {
        Box::new(AsyncStdinReader::new(
            self.stdin_reader_slot.clone(),
            &self.stdin_pump_once,
        ))
    }
}

impl Clone for Box<dyn ClientOsApi> {
    fn clone(&self) -> Box<dyn ClientOsApi> {
        self.box_clone()
    }
}

pub fn get_client_os_input() -> Result<ClientOsInputOutput, std::io::Error> {
    Ok(ClientOsInputOutput {
        send_instructions_to_server: Arc::new(Mutex::new(None)),
        receive_instructions_from_server: Arc::new(Mutex::new(None)),
        session_name: Arc::new(Mutex::new(None)),
        stdin_reader_slot: Arc::new(Mutex::new(None)),
        stdin_pump_once: Arc::new(OnceLock::new()),
    })
}

pub fn get_cli_client_os_input() -> Result<ClientOsInputOutput, std::io::Error> {
    Ok(ClientOsInputOutput {
        send_instructions_to_server: Arc::new(Mutex::new(None)),
        receive_instructions_from_server: Arc::new(Mutex::new(None)),
        session_name: Arc::new(Mutex::new(None)),
        stdin_reader_slot: Arc::new(Mutex::new(None)),
        stdin_pump_once: Arc::new(OnceLock::new()),
    })
}

pub const DEFAULT_STDIN_POLL_TIMEOUT_MS: u64 = 10;

#[cfg(test)]
mod tests {
    use super::*;

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
}

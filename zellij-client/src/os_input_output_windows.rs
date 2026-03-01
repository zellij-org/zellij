use crate::os_input_output::SignalEvent;

use anyhow::{Context, Result};
use async_trait::async_trait;

use std::io;
use std::io::Write;
use std::path::Path;
use zellij_utils::ipc::{IpcReceiverWithContext, IpcSenderWithContext};

/// Windows async signal listener.
///
/// Polls `crossterm::terminal::size()` at 100ms intervals for resize events,
/// and listens to `tokio::signal::windows` for ctrl_c/ctrl_break/ctrl_close.
pub(crate) struct AsyncSignalListener {
    interval: tokio::time::Interval,
    last_size: (u16, u16),
    ctrl_c: tokio::signal::windows::CtrlC,
    ctrl_break: tokio::signal::windows::CtrlBreak,
    ctrl_close: tokio::signal::windows::CtrlClose,
}

impl AsyncSignalListener {
    pub fn new() -> io::Result<Self> {
        let size = crossterm::terminal::size().unwrap_or((80, 24));
        Ok(Self {
            interval: tokio::time::interval(std::time::Duration::from_millis(100)),
            last_size: size,
            ctrl_c: tokio::signal::windows::ctrl_c()?,
            ctrl_break: tokio::signal::windows::ctrl_break()?,
            ctrl_close: tokio::signal::windows::ctrl_close()?,
        })
    }
}

#[async_trait]
impl crate::os_input_output::AsyncSignals for AsyncSignalListener {
    async fn recv(&mut self) -> Option<SignalEvent> {
        loop {
            tokio::select! {
                _ = self.interval.tick() => {
                    if let Ok(new_size) = crossterm::terminal::size() {
                        if new_size != self.last_size {
                            self.last_size = new_size;
                            return Some(SignalEvent::Resize);
                        }
                    }
                }
                result = self.ctrl_c.recv() => {
                    return result.map(|_| SignalEvent::Quit);
                }
                result = self.ctrl_break.recv() => {
                    return result.map(|_| SignalEvent::Quit);
                }
                result = self.ctrl_close.recv() => {
                    return result.map(|_| SignalEvent::Quit);
                }
            }
        }
    }
}

/// Windows blocking signal iterator.
///
/// Uses `SetConsoleCtrlHandler` with an `AtomicBool` for quit signals.
/// For resize detection, operates in two modes:
/// - **Channel mode**: receives resize notifications forwarded from the stdin
///   thread (which gets `Event::Resize` from crossterm). Much more responsive
///   than polling.
/// - **Poll fallback**: polls `crossterm::terminal::size()` at 50ms intervals.
///   Used when no receiver is provided or when the sender is dropped (VT reader
///   path).
pub(crate) struct BlockingSignalIterator {
    last_size: (u16, u16),
    resize_receiver: Option<std::sync::mpsc::Receiver<()>>,
}

mod win_ctrl_handler {
    use std::sync::atomic::{AtomicBool, Ordering};

    use windows_sys::Win32::Foundation::BOOL;
    use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT};

    pub static CTRL_QUIT_RECEIVED: AtomicBool = AtomicBool::new(false);

    pub unsafe extern "system" fn ctrl_handler(ctrl_type: u32) -> BOOL {
        match ctrl_type {
            CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT => {
                CTRL_QUIT_RECEIVED.store(true, Ordering::SeqCst);
                1 // TRUE — handled
            },
            _ => 0, // FALSE — not handled
        }
    }
}

impl BlockingSignalIterator {
    pub fn new(resize_receiver: Option<std::sync::mpsc::Receiver<()>>) -> io::Result<Self> {
        use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

        win_ctrl_handler::CTRL_QUIT_RECEIVED.store(false, std::sync::atomic::Ordering::SeqCst);

        let ok = unsafe { SetConsoleCtrlHandler(Some(win_ctrl_handler::ctrl_handler), 1) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }

        let size = crossterm::terminal::size().unwrap_or((80, 24));
        Ok(Self {
            last_size: size,
            resize_receiver,
        })
    }
}

impl Iterator for BlockingSignalIterator {
    type Item = SignalEvent;

    fn next(&mut self) -> Option<SignalEvent> {
        use std::sync::mpsc::RecvTimeoutError;
        use std::time::Duration;

        // Channel mode: block on receiver with timeout, check quit flag on each
        // iteration. If the sender disconnects (VT reader dropped it), fall
        // through to poll mode.
        if let Some(ref rx) = self.resize_receiver {
            loop {
                if win_ctrl_handler::CTRL_QUIT_RECEIVED.load(std::sync::atomic::Ordering::SeqCst) {
                    return Some(SignalEvent::Quit);
                }

                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(()) => return Some(SignalEvent::Resize),
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => {
                        // Sender dropped — switch to poll mode
                        break;
                    },
                }
            }
            self.resize_receiver = None;
        }

        // Poll fallback: same as the original implementation.
        loop {
            if win_ctrl_handler::CTRL_QUIT_RECEIVED.load(std::sync::atomic::Ordering::SeqCst) {
                return Some(SignalEvent::Quit);
            }

            if let Ok(new_size) = crossterm::terminal::size() {
                if new_size != self.last_size {
                    self.last_size = new_size;
                    return Some(SignalEvent::Resize);
                }
            }

            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// Set up client IPC channels from a connected socket.
///
/// On Windows we use two separate named pipes to avoid DuplicateHandle
/// deadlock: the command pipe (socket) for client→server, and a reply pipe
/// for server→client.
pub(crate) fn setup_ipc(
    socket: interprocess::local_socket::Stream,
    path: &Path,
) -> (
    IpcSenderWithContext<zellij_utils::ipc::ClientToServerMsg>,
    IpcReceiverWithContext<zellij_utils::ipc::ServerToClientMsg>,
) {
    let reply_socket;
    loop {
        match zellij_utils::consts::ipc_connect_reply(path) {
            Ok(sock) => {
                reply_socket = sock;
                break;
            },
            Err(_) => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            },
        }
    }
    let sender = IpcSenderWithContext::new(socket);
    let receiver = IpcReceiverWithContext::new(reply_socket);
    (sender, receiver)
}

/// Enable ENABLE_VIRTUAL_TERMINAL_PROCESSING on stdout so that ConPTY enters
/// passthrough mode and forwards DEC private mode sequences (like mouse-enable)
/// to the terminal emulator.  Uses crossterm's safe wrapper which handles the
/// GetConsoleMode/SetConsoleMode internally.
fn enable_vt_processing_on_stdout() {
    crossterm::ansi_support::supports_ansi();
}

/// Enable mouse support on Windows.
///
/// When TERM is set we're on the VT input path (terminal emulator like
/// Alacritty via ConPTY). We must NOT use crossterm's EnableMouseCapture
/// because it does a full SetConsoleMode() that would overwrite the mode
/// set by enable_vt_input(), clobbering ENABLE_VIRTUAL_TERMINAL_INPUT.
///
/// Instead, we enable ENABLE_VIRTUAL_TERMINAL_PROCESSING on stdout so
/// ConPTY enters passthrough mode, then write ANSI mouse-enable sequences.
///
/// When TERM is not set we're in a native console (cmd, PowerShell,
/// Windows Terminal) and use crossterm's Console API approach.
pub(crate) fn enable_mouse_support(stdout: &mut dyn Write) -> Result<()> {
    let err_context = "failed to enable mouse mode";
    if std::env::var("TERM").is_ok() {
        enable_vt_processing_on_stdout();
        stdout
            .write_all(super::os_input_output::ENABLE_MOUSE_SUPPORT.as_bytes())
            .context(err_context)?;
        stdout.flush().context(err_context)?;
    } else {
        // crossterm::execute! requires Sized, so we use std::io::stdout()
        // directly rather than the trait-object writer.
        crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture)
            .context(err_context)?;
    }
    Ok(())
}

/// Disable mouse support on Windows.
///
/// See `enable_mouse_support()` for rationale on VT vs Console API paths.
pub(crate) fn disable_mouse_support(stdout: &mut dyn Write) -> Result<()> {
    let err_context = "failed to disable mouse mode";
    if std::env::var("TERM").is_ok() {
        stdout
            .write_all(super::os_input_output::DISABLE_MOUSE_SUPPORT.as_bytes())
            .context(err_context)?;
        stdout.flush().context(err_context)?;
    } else {
        crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture)
            .context(err_context)?;
    }
    Ok(())
}

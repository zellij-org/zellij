use crate::os_input_output::SignalEvent;

use async_trait::async_trait;

use std::io;

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
                if win_ctrl_handler::CTRL_QUIT_RECEIVED
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
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

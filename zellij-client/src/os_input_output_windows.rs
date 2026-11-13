use crate::os_input_output::SignalEvent;
use crate::stdin_handler_windows::restore_vt_input;

use anyhow::{Context, Result};
use async_trait::async_trait;

use std::io;
use std::io::Write;
use std::path::Path;
use zellij_utils::ipc::{IpcReceiverWithContext, IpcSenderWithContext};

/// Whether zellij should use the VT byte path on Windows (raw stdin via
/// `ReadFile` + termwiz/kitty parsing) instead of the native-console path
/// (crossterm `INPUT_RECORD`s).
///
/// True when either of these environment variables is set:
///
/// * `TERM` — set automatically by terminal emulators that expose a Unix-style
///   environment (Alacritty, WezTerm, WSL sessions inheriting bash's env), or
///   set manually by users who want zellij on the VT path (e.g. adding
///   `TERM=xterm-256color` to a Windows Terminal / PowerShell profile).
/// * `WT_SESSION` — set per-session by Windows Terminal itself. Detecting it
///   means default WT installs land on the VT path without the user having to
///   set `TERM`, so they get bracketed paste, kitty keyboard, SGR mouse, and
///   the other features that depend on raw VT input.
///
/// Every VT-path-conditional code site (stdin handler, mouse mode setup,
/// future input-related code) should call this so the gates can't drift —
/// when they did drift, mouse setup using `crossterm::EnableMouseCapture`
/// clobbered the `ENABLE_VIRTUAL_TERMINAL_INPUT` flag the stdin path had
/// just set and broke all input from WT.
pub(crate) fn use_vt_path() -> bool {
    std::env::var("TERM").is_ok() || std::env::var("WT_SESSION").is_ok()
}

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

        loop {
            if win_ctrl_handler::CTRL_QUIT_RECEIVED.load(std::sync::atomic::Ordering::SeqCst) {
                return Some(SignalEvent::Quit);
            }

            if let Some(ref rx) = self.resize_receiver {
                // Channel mode: the native console stdin loop sends resize
                // notifications through this channel. Block with a timeout so
                // we can periodically check the quit flag above.
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(()) => return Some(SignalEvent::Resize),
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => {
                        // Sender dropped (VT reader path) — switch to poll mode.
                        self.resize_receiver = None;
                        continue;
                    },
                }
            } else {
                // Poll mode: used on the VT reader path where crossterm's
                // event::read() isn't used, so resize events don't come through
                // the channel. Periodically compare the terminal size instead.
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
}

/// Connects to the server at `path`, waiting for it to start listening.
///
/// On Windows we use two separate named pipes to avoid DuplicateHandle
/// deadlock: the command pipe (socket) for client→server, and a reply pipe
/// for server→client. Both are connected together, so the server can pair them.
pub(crate) fn connect_ipc(
    path: &Path,
) -> std::io::Result<(
    IpcSenderWithContext<zellij_utils::ipc::ClientToServerMsg>,
    IpcReceiverWithContext<zellij_utils::ipc::ServerToClientMsg>,
)> {
    let (socket, reply_socket) = loop {
        match zellij_utils::pipe_pairs::connect_pair(path) {
            Ok(pipes) => break pipes,
            // The session belongs to another user, or was started elevated: waiting won't help.
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Err(e),
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    };
    let sender = IpcSenderWithContext::new(socket);
    let receiver = IpcReceiverWithContext::new(reply_socket);
    Ok((sender, receiver))
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
    if use_vt_path() {
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

/// Restore the console input mode to its pre-Zellij state.
///
/// On the VT path, `enable_vt_input()` sets ENABLE_MOUSE_INPUT and
/// ENABLE_VIRTUAL_TERMINAL_INPUT on the console handle, but crossterm's
/// `disable_raw_mode()` never clears them.  This function restores the
/// original console mode saved before those flags were set.
pub(crate) fn restore_console_mode() {
    restore_vt_input();
}

/// Disable mouse support on Windows.
///
/// See `enable_mouse_support()` for rationale on VT vs Console API paths.
pub(crate) fn disable_mouse_support(stdout: &mut dyn Write) -> Result<()> {
    let err_context = "failed to disable mouse mode";
    if use_vt_path() {
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

/// Maximum chunk size for stdout writes on Windows (64 KB).
///
/// Windows Console and Named Pipes can fail with ERROR_NO_SYSTEM_RESOURCES (OS error 1450)
/// if a single I/O operation attempts to write more than available non-paged pool memory,
/// which typically manifests on buffers larger than 64 KB.
pub const STDOUT_CHUNK_SIZE: usize = 65536;

/// Smallest chunk the 1450 back-off will shrink to.
const MIN_STDOUT_CHUNK_SIZE: usize = 4096;

/// Writes all of `bytes` in chunks of at most `STDOUT_CHUNK_SIZE`.
///
/// Progress is tracked through `write()` so that a partial write followed by
/// ERROR_NO_SYSTEM_RESOURCES (1450) never re-sends bytes that already reached
/// the writer; after a 1450 the chunk size is halved for the rest of the call.
pub(crate) fn write_all_chunked(w: &mut dyn io::Write, mut bytes: &[u8]) -> io::Result<()> {
    let mut chunk_size = STDOUT_CHUNK_SIZE;
    while !bytes.is_empty() {
        let to_write = std::cmp::min(bytes.len(), chunk_size);
        match w.write(&bytes[..to_write]) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(written) => {
                bytes = &bytes[written..];
                let _ = w.flush();
            },
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {},
            Err(e) if e.raw_os_error() == Some(1450) && to_write > MIN_STDOUT_CHUNK_SIZE => {
                chunk_size = to_write / 2;
            },
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// A wrapper around a writer that chunks all writes to at most 64 KB on Windows,
/// avoiding ERROR_NO_SYSTEM_RESOURCES (1450).
pub struct ChunkedStdoutWriter<W: io::Write> {
    inner: W,
}

impl<W: io::Write> ChunkedStdoutWriter<W> {
    pub fn new(inner: W) -> Self {
        Self { inner }
    }
}

impl<W: io::Write> io::Write for ChunkedStdoutWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let to_write = std::cmp::min(buf.len(), STDOUT_CHUNK_SIZE);
        match self.inner.write(&buf[..to_write]) {
            Ok(n) => Ok(n),
            Err(e) if e.raw_os_error() == Some(1450) && to_write > 4096 => {
                self.inner.write(&buf[..to_write / 2])
            },
            Err(e) => Err(e),
        }
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        write_all_chunked(&mut self.inner, buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.inner.flush() {
            Ok(()) => Ok(()),
            Err(e) if e.raw_os_error() == Some(1450) => {
                std::thread::yield_now();
                self.inner.flush()
            },
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct RecordingWriter {
        writes: Vec<usize>,
        output: Vec<u8>,
    }

    impl RecordingWriter {
        fn new() -> Self {
            Self {
                writes: Vec::new(),
                output: Vec::new(),
            }
        }
    }

    impl io::Write for RecordingWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.writes.push(buf.len());
            self.output.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_chunked_stdout_writer_splits_large_writes() {
        let recorder = RecordingWriter::new();
        let mut chunked = ChunkedStdoutWriter::new(recorder);

        let data = vec![b'a'; 150_000]; // 150 KB > 64 KB
        chunked.write_all(&data).unwrap();

        assert_eq!(chunked.inner.output, data);
        assert!(chunked.inner.writes.len() >= 3);
        for &write_len in &chunked.inner.writes {
            assert!(write_len <= STDOUT_CHUNK_SIZE);
        }
    }

    /// Replays a script of outcomes, one per `write()` call, then accepts everything.
    enum Step {
        Partial(usize),
        Os1450,
        Interrupted,
        Zero,
    }

    struct ScriptedWriter {
        script: std::collections::VecDeque<Step>,
        output: Vec<u8>,
        sizes: Vec<usize>,
    }

    impl ScriptedWriter {
        fn new(script: Vec<Step>) -> Self {
            Self {
                script: script.into(),
                output: Vec::new(),
                sizes: Vec::new(),
            }
        }
    }

    impl io::Write for ScriptedWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.sizes.push(buf.len());
            let take = match self.script.pop_front() {
                Some(Step::Partial(n)) => n.min(buf.len()),
                Some(Step::Os1450) => return Err(io::Error::from_raw_os_error(1450)),
                Some(Step::Interrupted) => return Err(io::ErrorKind::Interrupted.into()),
                Some(Step::Zero) => return Ok(0),
                None => buf.len(),
            };
            self.output.extend_from_slice(&buf[..take]);
            Ok(take)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn partial_write_then_1450_does_not_repeat_bytes() {
        let data = pattern(150_000);
        let mut writer = ScriptedWriter::new(vec![Step::Partial(10_000), Step::Os1450]);
        write_all_chunked(&mut writer, &data).unwrap();
        assert_eq!(writer.output, data);
        // after the 1450 on a request of STDOUT_CHUNK_SIZE bytes the chunk size halves
        assert!(writer.sizes[2..]
            .iter()
            .all(|&s| s <= STDOUT_CHUNK_SIZE / 2));
    }

    #[test]
    fn repeated_1450_backs_off_to_the_floor_then_fails() {
        let data = pattern(STDOUT_CHUNK_SIZE);
        let script = (0..16).map(|_| Step::Os1450).collect();
        let mut writer = ScriptedWriter::new(script);
        let err = write_all_chunked(&mut writer, &data).unwrap_err();
        assert_eq!(err.raw_os_error(), Some(1450));
        assert_eq!(writer.sizes.last().copied(), Some(MIN_STDOUT_CHUNK_SIZE));
        assert!(writer.output.is_empty());
    }

    #[test]
    fn interrupted_is_retried_and_zero_write_is_an_error() {
        let data = pattern(5_000);
        let mut writer = ScriptedWriter::new(vec![Step::Interrupted, Step::Partial(1)]);
        write_all_chunked(&mut writer, &data).unwrap();
        assert_eq!(writer.output, data);

        let mut zero = ScriptedWriter::new(vec![Step::Zero]);
        let err = write_all_chunked(&mut zero, &data).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::WriteZero);
    }

    #[test]
    fn chunk_boundaries_are_preserved() {
        for len in [
            0,
            1,
            4096,
            4097,
            STDOUT_CHUNK_SIZE - 1,
            STDOUT_CHUNK_SIZE,
            STDOUT_CHUNK_SIZE + 1,
        ] {
            let data = pattern(len);
            let mut writer = ScriptedWriter::new(vec![]);
            write_all_chunked(&mut writer, &data).unwrap();
            assert_eq!(writer.output, data, "len {len}");
        }
    }

    #[test]
    fn wrapper_write_all_shares_the_helper_behaviour() {
        let data = pattern(70_000);
        let mut wrapped = ChunkedStdoutWriter::new(ScriptedWriter::new(vec![
            Step::Partial(3_000),
            Step::Os1450,
        ]));
        io::Write::write_all(&mut wrapped, &data).unwrap();
        assert_eq!(wrapped.inner.output, data);
    }
}

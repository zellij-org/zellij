use crate::os_input_output::{
    resolve_command, AsyncReader, PaneEnv, TerminalRunChangedBeforeWrite,
};
use crate::panes::PaneId;

use std::{
    collections::{BTreeMap, HashMap},
    ffi::OsStr,
    io,
    os::windows::ffi::OsStrExt,
    os::windows::io::{FromRawHandle, IntoRawHandle, OwnedHandle},
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc, Mutex, Weak,
    },
    time::{Duration, Instant},
};

use tokio::net::windows::named_pipe::NamedPipeServer;

use windows_sys::Win32::Foundation::{
    CloseHandle, DuplicateHandle, DUPLICATE_SAME_ACCESS, ERROR_IO_PENDING, ERROR_NOT_FOUND,
    ERROR_OPERATION_ABORTED, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    STILL_ACTIVE, S_OK, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FlushFileBuffers, WriteFile, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
    PIPE_ACCESS_INBOUND, PIPE_ACCESS_OUTBOUND,
};
use windows_sys::Win32::System::Console::{
    ClosePseudoConsole, CreatePseudoConsole, GenerateConsoleCtrlEvent, ResizePseudoConsole, COORD,
    CTRL_C_EVENT, HPCON,
};
use windows_sys::Win32::System::Pipes::CreateNamedPipeW;
use windows_sys::Win32::System::Threading::{
    CreateEventW, CreateProcessW, DeleteProcThreadAttributeList, GetCurrentProcess,
    GetExitCodeProcess, InitializeProcThreadAttributeList, TerminateProcess,
    UpdateProcThreadAttribute, WaitForSingleObject, CREATE_UNICODE_ENVIRONMENT,
    EXTENDED_STARTUPINFO_PRESENT, INFINITE, PROCESS_INFORMATION, STARTF_USESTDHANDLES,
    STARTUPINFOEXW, STARTUPINFOW,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

use zellij_utils::{errors::prelude::*, input::command::RunCommand};

pub use async_trait::async_trait;

// Not exported by windows-sys; value from the Windows SDK.
const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x00020016;
const FILE_FLAG_FIRST_PIPE_INSTANCE: u32 = 0x00080000;
const PIPE_TYPE_BYTE: u32 = 0;
const PIPE_WAIT: u32 = 0;

/// Bound on one ConPTY input write. A child that stops reading must not pin
/// the shared pty-writer thread; when this expires the overlapped write is
/// cancelled and its storage is reclaimed asynchronously.
const CONPTY_INPUT_WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Monotonic counter so each ConPTY output pipe gets a unique name, even when
/// re-spawning on the same `terminal_id` (the old async reader may still hold
/// the previous pipe handle).
static PIPE_SEQ: AtomicU64 = AtomicU64::new(0);
static TERMINAL_INCARNATION_SEQ: AtomicU64 = AtomicU64::new(0);

/// Per-terminal ConPTY state.
struct ConPtyTerminal {
    hpcon: HPCON,
    input_write_handle: HANDLE,
    /// Distinguishes a replacement console that reuses the same terminal id.
    incarnation: u64,
    /// `None` until the run that owns this console is bound. A stamped
    /// write is refused until it matches.
    generation: Option<u64>,
    /// Process handle from `CreateProcessW`, kept until this run is dropped.
    /// Terminate uses it. Closing it and reopening the numeric pid can signal
    /// a process that reused the id.
    process_handle: HANDLE,
    child_pid: u32,
}

// HANDLE/HPCON are raw pointers in windows-sys >= 0.59; OS handles are
// safe to send across threads.
unsafe impl Send for ConPtyTerminal {}
unsafe impl Sync for ConPtyTerminal {}

impl Drop for ConPtyTerminal {
    fn drop(&mut self) {
        unsafe {
            // Close the pseudo console first — it may write a final VT frame
            // to the output pipe. The async reader task (if still alive) will
            // drain it. Then close the remaining handle.
            ClosePseudoConsole(self.hpcon);
            CloseHandle(self.input_write_handle);
            if !self.process_handle.is_null() {
                CloseHandle(self.process_handle);
            }
        }
    }
}

/// Closes a duplicated handle. The original console handle stays open.
struct CloseOnDrop(HANDLE);

impl Drop for CloseOnDrop {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

fn duplicate_write_handle(handle: HANDLE) -> io::Result<HANDLE> {
    let mut duplicated: HANDLE = std::ptr::null_mut();
    let ok = unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            handle,
            GetCurrentProcess(),
            &mut duplicated,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(duplicated)
    }
}

/// One overlapped ConPTY input write. The buffer and `OVERLAPPED` are owned
/// here until the kernel has finished with them, including after
/// `CancelIoEx`, so the shared pty writer never has to wait for a stalled
/// terminal.
struct PendingOverlappedWrite {
    overlapped: Box<OVERLAPPED>,
    event: HANDLE,
    handle: HANDLE,
    #[allow(dead_code)]
    buffer: Vec<u8>,
    started_at: Instant,
    cancelled: bool,
    generation: Option<u64>,
    incarnation: u64,
}

// Raw OS handles are safe to move between threads; the kernel operation and
// its storage are owned by this struct for the whole lifetime of the I/O.
unsafe impl Send for PendingOverlappedWrite {}

impl PendingOverlappedWrite {
    fn poll(&mut self) -> Option<(usize, io::Result<()>)> {
        match unsafe { WaitForSingleObject(self.event, 0) } {
            WAIT_OBJECT_0 => {
                let mut transferred: u32 = 0;
                if unsafe {
                    GetOverlappedResult(self.handle, &*self.overlapped, &mut transferred, 0)
                } == 0
                {
                    let error = io::Error::last_os_error();
                    if error.raw_os_error() == Some(ERROR_OPERATION_ABORTED as i32) {
                        Some((
                            transferred as usize,
                            Err(io::Error::new(
                                io::ErrorKind::TimedOut,
                                "ConPTY input write timed out",
                            )),
                        ))
                    } else {
                        Some((transferred as usize, Err(error)))
                    }
                } else {
                    Some((transferred as usize, Ok(())))
                }
            },
            WAIT_TIMEOUT => None,
            WAIT_FAILED => Some((0, Err(io::Error::last_os_error()))),
            other => Some((
                0,
                Err(io::Error::other(format!(
                    "unexpected wait result {}",
                    other
                ))),
            )),
        }
    }

    fn cancel(&mut self) {
        if self.cancelled {
            return;
        }
        if unsafe { CancelIoEx(self.handle, &*self.overlapped) } != 0 {
            self.cancelled = true;
            return;
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_NOT_FOUND as i32) {
            // The operation already completed; there is nothing to cancel.
            self.cancelled = true;
        } else {
            log::warn!("CancelIoEx failed for a ConPTY input write: {error}");
        }
    }
}

impl Drop for PendingOverlappedWrite {
    fn drop(&mut self) {
        let completed = unsafe {
            if !self.cancelled {
                let _ = CancelIoEx(self.handle, &*self.overlapped);
            }
            WaitForSingleObject(self.event, 1000) == WAIT_OBJECT_0
        };
        if completed {
            unsafe {
                let mut transferred: u32 = 0;
                let _ = GetOverlappedResult(self.handle, &*self.overlapped, &mut transferred, 0);
                if !self.event.is_null() {
                    CloseHandle(self.event);
                }
                if !self.handle.is_null() {
                    CloseHandle(self.handle);
                }
            }
        } else {
            // The driver never completed cancellation. Freeing the storage
            // now would let the kernel write into released memory, so leak the
            // kernel-visible pieces instead. This is teardown, not the shared
            // writer path, and should be exceptional.
            log::error!("leaking a ConPTY overlapped write that did not complete cancellation");
            let overlapped = std::mem::replace(
                &mut self.overlapped,
                Box::new(unsafe { std::mem::zeroed() }),
            );
            std::mem::forget(overlapped);
            std::mem::forget(std::mem::take(&mut self.buffer));
            self.event = std::ptr::null_mut();
            self.handle = std::ptr::null_mut();
        }
    }
}

fn start_overlapped_write(
    handle: HANDLE,
    buf: &[u8],
    generation: Option<u64>,
    incarnation: u64,
) -> (usize, io::Result<()>, Option<PendingOverlappedWrite>) {
    if buf.is_empty() {
        return (0, Ok(()), None);
    }
    let len = match u32::try_from(buf.len()) {
        Ok(len) => len,
        Err(_) => {
            return (
                0,
                Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "ConPTY input chunk exceeds u32",
                )),
                None,
            );
        },
    };
    let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    if event.is_null() {
        return (0, Err(io::Error::last_os_error()), None);
    }
    let mut overlapped: Box<OVERLAPPED> = Box::new(unsafe { std::mem::zeroed() });
    overlapped.hEvent = event;
    let buffer = buf.to_vec();
    let mut written: u32 = 0;
    if unsafe { WriteFile(handle, buffer.as_ptr(), len, &mut written, &mut *overlapped) } != 0 {
        unsafe { CloseHandle(event) };
        return (written as usize, Ok(()), None);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
        unsafe { CloseHandle(event) };
        return (0, Err(error), None);
    }

    (
        0,
        Ok(()),
        Some(PendingOverlappedWrite {
            overlapped,
            event,
            handle,
            buffer,
            started_at: Instant::now(),
            cancelled: false,
            generation,
            incarnation,
        }),
    )
}

/// Cancel operations that exceeded their deadline and reclaim retired
/// operations once the kernel releases their storage. This is one thread for
/// the whole backend, not one per write.
fn spawn_overlapped_reaper(
    terminals: Weak<Mutex<BTreeMap<u32, Option<ConPtyTerminal>>>>,
    pending: Weak<Mutex<HashMap<u32, PendingOverlappedWrite>>>,
    retired: Weak<Mutex<Vec<PendingOverlappedWrite>>>,
) -> io::Result<()> {
    std::thread::Builder::new()
        .name("conpty-write-reaper".to_owned())
        .spawn(move || loop {
            let (Some(terminals), Some(pending), Some(retired)) =
                (terminals.upgrade(), pending.upgrade(), retired.upgrade())
            else {
                break;
            };
            let live_incarnations: HashMap<u32, u64> = {
                let terminals = terminals.lock().unwrap_or_else(|error| error.into_inner());
                terminals
                    .iter()
                    .filter_map(|(terminal_id, terminal)| {
                        terminal
                            .as_ref()
                            .map(|terminal| (*terminal_id, terminal.incarnation))
                    })
                    .collect()
            };
            let mut newly_retired = Vec::new();
            {
                let mut pending = pending.lock().unwrap_or_else(|error| error.into_inner());
                let stale: Vec<u32> = pending
                    .iter()
                    .filter(|(terminal_id, write)| {
                        live_incarnations.get(terminal_id) != Some(&write.incarnation)
                    })
                    .map(|(terminal_id, _)| *terminal_id)
                    .collect();
                for terminal_id in stale {
                    if let Some(mut write) = pending.remove(&terminal_id) {
                        write.cancel();
                        newly_retired.push(write);
                    }
                }
                for write in pending.values_mut() {
                    if !write.cancelled && write.started_at.elapsed() >= CONPTY_INPUT_WRITE_TIMEOUT
                    {
                        write.cancel();
                    }
                }
            }
            if !newly_retired.is_empty() {
                let mut retired = retired.lock().unwrap_or_else(|error| error.into_inner());
                retired.append(&mut newly_retired);
            }
            let completed: Vec<PendingOverlappedWrite> = {
                let mut retired = retired.lock().unwrap_or_else(|error| error.into_inner());
                let mut completed = Vec::new();
                let mut index = 0;
                while index < retired.len() {
                    if retired[index].poll().is_some() {
                        completed.push(retired.swap_remove(index));
                    } else {
                        index += 1;
                    }
                }
                completed
            };
            drop(completed);
            drop(terminals);
            drop(pending);
            drop(retired);
            std::thread::sleep(Duration::from_millis(25));
        })
        .map(|_| ())
}

/// Cancel an operation whose result no longer matters. The reaper reclaims
/// its storage once the kernel completes the cancellation.
fn retire_overlapped_write(
    entry: PendingOverlappedWrite,
    retired: &Mutex<Vec<PendingOverlappedWrite>>,
) {
    let mut retired = retired.lock().unwrap_or_else(|error| error.into_inner());
    retired.push(entry);
}

/// An `AsyncReader` backed by a named pipe connected to ConPTY output.
///
/// Construction stores the raw `OwnedHandle`. The first `read()` call promotes
/// it to a `NamedPipeServer` (IOCP registration requires a live Tokio reactor,
/// which is not available at `spawn_terminal` time).
struct ConPtyAsyncReader {
    pending: Option<OwnedHandle>,
    pipe: Option<NamedPipeServer>,
}

// OwnedHandle is Send+Sync; NamedPipeServer is Send.
// The reader is only ever used from a single async task (via &mut self),
// so Sync is safe.
unsafe impl Sync for ConPtyAsyncReader {}

impl ConPtyAsyncReader {
    fn new(handle: OwnedHandle) -> Self {
        Self {
            pending: Some(handle),
            pipe: None,
        }
    }
}

#[async_trait]
impl AsyncReader for ConPtyAsyncReader {
    async fn read_chunk(&mut self, max: usize) -> Result<Vec<u8>, io::Error> {
        let pipe = self.pipe()?;
        loop {
            pipe.readable().await?;
            let mut buf = vec![0u8; max];
            match pipe.try_read(&mut buf) {
                Ok(n) => return Ok(buf[..n].to_vec()),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(e) => return Err(e),
            }
        }
    }
}

impl ConPtyAsyncReader {
    fn pipe(&mut self) -> Result<&mut NamedPipeServer, io::Error> {
        if let Some(handle) = self.pending.take() {
            let pipe = unsafe { NamedPipeServer::from_raw_handle(handle.into_raw_handle()) }?;
            self.pipe = Some(pipe);
        }
        Ok(self
            .pipe
            .as_mut()
            .expect("ConPtyAsyncReader used after init"))
    }
}

// ---------------------------------------------------------------------------
// Helper utilities
// ---------------------------------------------------------------------------

/// Encode a Rust string as null-terminated UTF-16.
fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Quote a single argument per the `CommandLineToArgvW` convention.
///
/// Rules:
///   * n backslashes not followed by a quote are n literal backslashes.
///   * 2n backslashes followed by a quote produce n backslashes and a
///     delimiting quote.
///   * 2n+1 backslashes followed by a quote produce n backslashes and
///     a literal quote.
fn append_quoted_arg(cmdline: &mut String, arg: &str) {
    if !(arg.is_empty() || arg.contains(' ') || arg.contains('\t') || arg.contains('"')) {
        cmdline.push_str(arg);
        return;
    }
    cmdline.push('"');
    let mut backslashes: usize = 0;
    for ch in arg.chars() {
        if ch == '\\' {
            backslashes += 1;
        } else if ch == '"' {
            for _ in 0..(backslashes * 2 + 1) {
                cmdline.push('\\');
            }
            backslashes = 0;
            cmdline.push('"');
        } else {
            for _ in 0..backslashes {
                cmdline.push('\\');
            }
            backslashes = 0;
            cmdline.push(ch);
        }
    }
    // Double any trailing backslashes so the closing quote still terminates
    // the argument rather than being escaped.
    for _ in 0..(backslashes * 2) {
        cmdline.push('\\');
    }
    cmdline.push('"');
}

/// Build a Windows command-line string from a `RunCommand`, following the
/// `CommandLineToArgvW` quoting convention.
fn build_command_line_string(cmd: &RunCommand) -> String {
    let mut cmdline = String::new();

    // Executable — always quote to handle spaces in paths.
    let exe = cmd.command.to_string_lossy();
    cmdline.push('"');
    cmdline.push_str(&exe);
    cmdline.push('"');

    for arg in &cmd.args {
        cmdline.push(' ');
        append_quoted_arg(&mut cmdline, arg);
    }

    cmdline
}

fn build_command_line(cmd: &RunCommand) -> Vec<u16> {
    to_wide(&build_command_line_string(cmd))
}

/// Build a UTF-16 environment block (each entry `KEY=VALUE\0`, terminated by
/// an extra `\0`) from the current process environment, adding
/// `ZELLIJ_PANE_ID`.
fn build_environment_block(terminal_id: u32, pane_env: &PaneEnv) -> Vec<u16> {
    let mut variables: BTreeMap<String, String> = std::env::vars()
        .filter(|(key, _)| !key.eq_ignore_ascii_case("ZELLIJ_PANE_ID"))
        .collect();
    for (key, value) in pane_env {
        variables.retain(|existing, _| !existing.eq_ignore_ascii_case(key));
        if let Some(value) = value {
            variables.insert(key.clone(), value.clone());
        }
    }
    let mut block: Vec<u16> = Vec::new();
    for (key, value) in variables {
        let entry = format!("{}={}", key, value);
        block.extend(OsStr::new(&entry).encode_wide());
        block.push(0);
    }
    let pane_entry = format!("ZELLIJ_PANE_ID={}", terminal_id);
    block.extend(OsStr::new(&pane_entry).encode_wide());
    block.push(0);
    block.push(0); // double-null terminator
    block
}

/// Create an overlapped named-pipe pair for ConPTY output.
///
/// Returns `(server_read_handle, client_write_handle)` where the server
/// (read) end has `FILE_FLAG_OVERLAPPED` for IOCP and the client (write) end
/// is synchronous (required by ConPTY).
fn create_overlapped_output_pipe(terminal_id: u32) -> io::Result<(HANDLE, HANDLE)> {
    let seq = PIPE_SEQ.fetch_add(1, Ordering::Relaxed);
    let name = format!(
        r"\\.\pipe\zellij-pty-{}-{}-{}",
        std::process::id(),
        terminal_id,
        seq
    );
    let wide_name = to_wide(&name);

    let server = unsafe {
        CreateNamedPipeW(
            wide_name.as_ptr(),
            PIPE_ACCESS_INBOUND | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_WAIT,
            1,     // max instances
            0,     // out buffer (we only read)
            65536, // in buffer
            0,     // default timeout
            std::ptr::null(),
        )
    };
    if server == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }

    let client = unsafe {
        CreateFileW(
            wide_name.as_ptr(),
            GENERIC_WRITE,
            0,                    // no sharing
            std::ptr::null(),     // default security
            OPEN_EXISTING,        // pipe already exists
            0,                    // synchronous
            std::ptr::null_mut(), // no template
        )
    };
    if client == INVALID_HANDLE_VALUE {
        unsafe { CloseHandle(server) };
        return Err(io::Error::last_os_error());
    }

    Ok((server, client))
}

/// Create an input pipe whose server (write) end is overlapped, so a blocked
/// write can be cancelled with `CancelIoEx`.
///
/// Returns `(client_read_handle, server_write_handle)`. The read end stays
/// synchronous because ConPTY consumes it; only the end this process writes
/// through needs overlapped I/O.
fn create_overlapped_input_pipe(terminal_id: u32) -> io::Result<(HANDLE, HANDLE)> {
    let seq = PIPE_SEQ.fetch_add(1, Ordering::Relaxed);
    let name = format!(
        r"\\.\pipe\zellij-pty-in-{}-{}-{}",
        std::process::id(),
        terminal_id,
        seq
    );
    let wide_name = to_wide(&name);

    let server = unsafe {
        CreateNamedPipeW(
            wide_name.as_ptr(),
            PIPE_ACCESS_OUTBOUND | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_WAIT,
            1,     // max instances
            65536, // out buffer (the end this process writes to)
            0,     // in buffer (we only write)
            0,     // default timeout
            std::ptr::null(),
        )
    };
    if server == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }

    let client = unsafe {
        CreateFileW(
            wide_name.as_ptr(),
            GENERIC_READ,
            0,                    // no sharing
            std::ptr::null(),     // default security
            OPEN_EXISTING,        // pipe already exists
            0,                    // synchronous
            std::ptr::null_mut(), // no template
        )
    };
    if client == INVALID_HANDLE_VALUE {
        unsafe { CloseHandle(server) };
        return Err(io::Error::last_os_error());
    }

    Ok((client, server))
}

/// Create a ConPTY pseudo console of the given size attached to the provided
/// pipes.
fn create_conpty(
    cols: u16,
    rows: u16,
    input_read: HANDLE,
    output_write: HANDLE,
) -> io::Result<HPCON> {
    let size = COORD {
        X: cols as i16,
        Y: rows as i16,
    };
    let mut hpcon: HPCON = 0;
    let hr = unsafe { CreatePseudoConsole(size, input_read, output_write, 0, &mut hpcon) };
    if hr != S_OK {
        Err(io::Error::from_raw_os_error(hr))
    } else {
        Ok(hpcon)
    }
}

/// Spawn a child process attached to the given ConPTY.
///
/// Returns `(process_handle, thread_handle, child_pid)`.
fn spawn_child_process(
    hpcon: HPCON,
    cmd: &RunCommand,
    terminal_id: u32,
    pane_env: &PaneEnv,
) -> io::Result<(HANDLE, HANDLE, u32)> {
    // --- proc thread attribute list ---
    let mut attr_size: usize = 0;
    unsafe {
        InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut attr_size);
    }
    let mut attr_buf = vec![0u8; attr_size];
    let attr_list = attr_buf.as_mut_ptr().cast();

    if unsafe { InitializeProcThreadAttributeList(attr_list, 1, 0, &mut attr_size) } == 0 {
        return Err(io::Error::last_os_error());
    }

    // N.B. For PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, lpValue is the HPCON
    // value itself (not a pointer to it). In C, HPCON is `void*` so passing
    // it directly as PVOID is natural. In Rust, HPCON is `isize`, so we cast
    // the value to a pointer. This matches the Microsoft ConPTY sample.
    // See: https://github.com/microsoft/terminal/issues/6705
    if unsafe {
        UpdateProcThreadAttribute(
            attr_list,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
            hpcon as *const core::ffi::c_void,
            std::mem::size_of::<HPCON>(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        unsafe { DeleteProcThreadAttributeList(attr_list) };
        return Err(io::Error::last_os_error());
    }

    // --- startup info ---
    let mut si: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    // Null standard handles make the child use the pseudo console. Without this flag a child
    // takes the server's own standard handles whenever those are redirected (as they are when
    // the server is started without a console), and a shell reading a NUL stdin exits at once.
    si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    si.lpAttributeList = attr_list;

    // --- command line & environment ---
    let mut cmd_line = build_command_line(cmd);
    let env_block = build_environment_block(terminal_id, pane_env);

    let cwd: Option<Vec<u16>> = cmd.cwd.as_ref().and_then(|p| {
        if p.exists() && p.is_dir() {
            Some(to_wide(&p.to_string_lossy()))
        } else {
            log::error!(
                "CWD for new pane '{}' does not exist or is not a directory",
                p.display()
            );
            None
        }
    });
    let cwd_ptr = cwd.as_ref().map_or(std::ptr::null(), |v| v.as_ptr());

    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    let ok = unsafe {
        CreateProcessW(
            std::ptr::null(),      // lpApplicationName
            cmd_line.as_mut_ptr(), // lpCommandLine (mutable)
            std::ptr::null(),      // lpProcessAttributes
            std::ptr::null(),      // lpThreadAttributes
            0,                     // bInheritHandles = FALSE
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
            env_block.as_ptr().cast(),              // lpEnvironment
            cwd_ptr,                                // lpCurrentDirectory
            &si.StartupInfo as *const STARTUPINFOW, // lpStartupInfo
            &mut pi,                                // lpProcessInformation
        )
    };

    unsafe { DeleteProcThreadAttributeList(attr_list) };

    if ok == 0 {
        return Err(io::Error::last_os_error());
    }

    Ok((pi.hProcess, pi.hThread, pi.dwProcessId))
}

fn terminate_handle(handle: HANDLE) -> io::Result<()> {
    let ok = unsafe { TerminateProcess(handle, 1) };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// WindowsPtyBackend
// ---------------------------------------------------------------------------

/// Windows PTY backend using native ConPTY with IOCP-based async I/O.
#[derive(Clone)]
pub(crate) struct WindowsPtyBackend {
    terminals: Arc<Mutex<BTreeMap<u32, Option<ConPtyTerminal>>>>,
    last_terminal_sizes: Arc<Mutex<BTreeMap<u32, (u16, u16)>>>,
    next_terminal_id_counter: Arc<AtomicU32>,
    /// At most one in-flight overlapped input write per terminal. The shared
    /// pty writer polls completion instead of waiting for a stuck ConPTY.
    pending_writes: Arc<Mutex<HashMap<u32, PendingOverlappedWrite>>>,
    /// Cancelled operations whose result is no longer needed. They keep their
    /// buffer and OVERLAPPED alive until the kernel releases them.
    retired_writes: Arc<Mutex<Vec<PendingOverlappedWrite>>>,
}

impl WindowsPtyBackend {
    pub fn new() -> Result<Self, io::Error> {
        let terminals = Arc::new(Mutex::new(BTreeMap::new()));
        let pending_writes = Arc::new(Mutex::new(HashMap::new()));
        let retired_writes = Arc::new(Mutex::new(Vec::new()));
        if let Err(error) = spawn_overlapped_reaper(
            Arc::downgrade(&terminals),
            Arc::downgrade(&pending_writes),
            Arc::downgrade(&retired_writes),
        ) {
            log::error!("failed to start the ConPTY write reaper: {error}");
        }
        Ok(Self {
            terminals,
            last_terminal_sizes: Arc::new(Mutex::new(BTreeMap::new())),
            next_terminal_id_counter: Arc::new(AtomicU32::new(0)),
            pending_writes,
            retired_writes,
        })
    }

    fn initial_size_for(&self, terminal_id: u32) -> (u16, u16) {
        if let Ok(sizes) = self.last_terminal_sizes.lock() {
            if let Some((cols, rows)) = sizes.get(&terminal_id) {
                if *cols > 0 && *rows > 0 {
                    return (*cols, *rows);
                }
            }
        }
        (80, 24)
    }

    /// Core spawn logic — creates ConPTY, spawns child, sets up exit monitor.
    fn do_spawn(
        &self,
        cmd: RunCommand,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        terminal_id: u32,
        pane_env: &PaneEnv,
    ) -> Result<(
        Box<dyn AsyncReader>,
        u32,
        Option<crate::os_input_output::ChildPidFd>,
    )> {
        let err_context = |c: &RunCommand| {
            format!(
                "failed to spawn terminal for '{}'",
                c.command.to_string_lossy()
            )
        };

        // 1. Output pipe pair (named, overlapped read end for IOCP)
        let (output_read, output_write) =
            create_overlapped_output_pipe(terminal_id).with_context(|| err_context(&cmd))?;

        // 2. Input pipe pair. The write end is overlapped so a blocked write
        // can be cancelled instead of pinning the pty-writer thread.
        let (input_read, input_write) = match create_overlapped_input_pipe(terminal_id) {
            Ok(pipe) => pipe,
            Err(error) => {
                unsafe {
                    CloseHandle(output_read);
                    CloseHandle(output_write);
                }
                return Err(error).with_context(|| err_context(&cmd));
            },
        };

        // 3. Create pseudo console with the last known size (or 80x24) so a
        // rerun never starts the child at an empty ConPTY.
        let (cols, rows) = self.initial_size_for(terminal_id);
        if let Ok(mut sizes) = self.last_terminal_sizes.lock() {
            sizes.insert(terminal_id, (cols, rows));
        }
        let hpcon = match create_conpty(cols, rows, input_read, output_write) {
            Ok(h) => h,
            Err(e) => {
                unsafe {
                    CloseHandle(output_read);
                    CloseHandle(output_write);
                    CloseHandle(input_read);
                    CloseHandle(input_write);
                }
                return Err(e).with_context(|| err_context(&cmd));
            },
        };

        // 4. ConPTY duplicated the pipe ends it needs; close our copies.
        unsafe {
            CloseHandle(input_read);
            CloseHandle(output_write);
        }

        // 5. Spawn child process
        let (process_handle, thread_handle, child_pid) =
            match spawn_child_process(hpcon, &cmd, terminal_id, pane_env) {
                Ok(r) => r,
                Err(e) => {
                    unsafe {
                        ClosePseudoConsole(hpcon);
                        CloseHandle(input_write);
                        CloseHandle(output_read);
                    }
                    return Err(e).with_context(|| err_context(&cmd));
                },
            };

        // Thread handle is not needed after spawn.
        unsafe { CloseHandle(thread_handle) };

        // The waiter gets a duplicate. The original stays on the terminal
        // until that run is dropped, so terminate does not reopen the pid.
        let waiter_handle = match duplicate_write_handle(process_handle) {
            Ok(handle) => handle,
            Err(e) => {
                unsafe {
                    CloseHandle(process_handle);
                    ClosePseudoConsole(hpcon);
                    CloseHandle(input_write);
                    CloseHandle(output_read);
                }
                return Err(e).with_context(|| err_context(&cmd));
            },
        };

        // 6. Store per-terminal state. Drop any replaced console only after
        // releasing the registry lock: ClosePseudoConsole can block while the
        // output pipe is still being drained on older Windows versions.
        let replaced_terminal = self.terminals.lock().unwrap().insert(
            terminal_id,
            Some(ConPtyTerminal {
                hpcon,
                input_write_handle: input_write,
                incarnation: TERMINAL_INCARNATION_SEQ.fetch_add(1, Ordering::Relaxed),
                generation: None,
                process_handle,
                child_pid,
            }),
        );
        drop(replaced_terminal);
        let replaced_pending = self
            .pending_writes
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&terminal_id);
        if let Some(mut replaced_pending) = replaced_pending {
            replaced_pending.cancel();
            retire_overlapped_write(replaced_pending, &self.retired_writes);
        }

        // 7. Exit-monitoring thread (zero CPU — spends all time in kernel wait).
        // Its duplicate is closed after `quit_cb`, not before: the callback
        // can still terminate through the handle stored above.
        let cmd_for_monitor = cmd.clone();
        // Raw handles are not `Send`. The address is, and the OS handle is
        // safe to use from the waiter thread.
        let waiter_handle_addr = waiter_handle as usize;
        std::thread::spawn(move || {
            let waiter_handle = waiter_handle_addr as HANDLE;
            let _close_waiter = CloseOnDrop(waiter_handle);
            let exit_code = unsafe {
                WaitForSingleObject(waiter_handle, INFINITE);
                let mut code: u32 = 0;
                GetExitCodeProcess(waiter_handle, &mut code);
                code
            };
            quit_cb(
                PaneId::Terminal(terminal_id),
                Some(exit_code as i32),
                cmd_for_monitor,
            );
        });

        // 8. Wrap the output read handle in an async reader
        let owned = unsafe { OwnedHandle::from_raw_handle(output_read as *mut core::ffi::c_void) };
        let reader = Box::new(ConPtyAsyncReader::new(owned)) as Box<dyn AsyncReader>;

        Ok((reader, child_pid, None))
    }

    pub fn spawn_terminal(
        &self,
        mut cmd: RunCommand,
        failover_cmd: Option<RunCommand>,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        terminal_id: u32,
        pane_env: &PaneEnv,
    ) -> Result<(
        Box<dyn AsyncReader>,
        u32,
        Option<crate::os_input_output::ChildPidFd>,
    )> {
        if let Some(resolved) = resolve_command(&cmd, pane_env) {
            cmd.command = resolved;
            return self.do_spawn(cmd, quit_cb, terminal_id, pane_env);
        }
        if let Some(mut failover) = failover_cmd {
            if let Some(resolved) = resolve_command(&failover, pane_env) {
                failover.command = resolved;
                return self.do_spawn(failover, quit_cb, terminal_id, pane_env);
            }
        }
        Err(ZellijError::CommandNotFound {
            terminal_id,
            command: cmd.command.to_string_lossy().to_string(),
        })
        .context("failed to spawn terminal")
    }

    pub fn set_terminal_size(
        &self,
        terminal_id: u32,
        cols: u16,
        rows: u16,
        _width_in_pixels: Option<u16>,
        _height_in_pixels: Option<u16>,
    ) -> Result<()> {
        let err_context = || {
            format!(
                "failed to set terminal {} to size ({}, {})",
                terminal_id, cols, rows
            )
        };

        match self
            .terminals
            .lock()
            .to_anyhow()
            .with_context(err_context)?
            .get(&terminal_id)
        {
            Some(Some(term)) => {
                if cols > 0 && rows > 0 {
                    let size = COORD {
                        X: cols as i16,
                        Y: rows as i16,
                    };
                    let hr = unsafe { ResizePseudoConsole(term.hpcon, size) };
                    if hr != S_OK {
                        Err::<(), _>(anyhow!("ResizePseudoConsole failed: HRESULT 0x{:08x}", hr))
                            .with_context(err_context)
                            .non_fatal();
                    } else if let Ok(mut sizes) = self.last_terminal_sizes.lock() {
                        sizes.insert(terminal_id, (cols, rows));
                    }
                }
            },
            _ => {
                // Remember the intended size even when ConPTY is not yet open
                // so the next spawn creates a nonzero pseudo console.
                if cols > 0 && rows > 0 {
                    if let Ok(mut sizes) = self.last_terminal_sizes.lock() {
                        sizes.insert(terminal_id, (cols, rows));
                    }
                }
                Err::<(), _>(anyhow!("no ConPTY terminal found for id {}", terminal_id))
                    .with_context(err_context)
                    .non_fatal();
            },
        }
        Ok(())
    }

    pub fn write_to_tty_stdin_for_run(
        &self,
        terminal_id: u32,
        generation: Option<u64>,
        buf: &[u8],
    ) -> Result<usize> {
        let (written, result) =
            self.write_to_tty_stdin_for_run_with_progress(terminal_id, generation, buf);
        result.map(|_| written)
    }

    pub fn write_to_tty_stdin_for_run_with_progress(
        &self,
        terminal_id: u32,
        generation: Option<u64>,
        buf: &[u8],
    ) -> (usize, Result<()>) {
        let err_context = || format!("failed to write to stdin of terminal {}", terminal_id);

        let mut retired_entry = None;
        let mut completed_entry = None;
        {
            let mut pending = self
                .pending_writes
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let mut generation_changed = false;
            if let Some(entry) = pending.get_mut(&terminal_id) {
                if generation.is_none() && entry.generation.is_some() {
                    return (
                        0,
                        Err(anyhow!(
                            "a run-stamped ConPTY input write is already in flight for terminal {}",
                            terminal_id
                        )),
                    );
                }
                if let (Some(expected), Some(existing)) = (generation, entry.generation) {
                    if existing != expected {
                        entry.cancel();
                        generation_changed = true;
                    }
                }
                if !generation_changed {
                    if !entry.cancelled && entry.started_at.elapsed() >= CONPTY_INPUT_WRITE_TIMEOUT
                    {
                        entry.cancel();
                    }
                    match entry.poll() {
                        None => return (0, Ok(())),
                        Some(result) => {
                            completed_entry =
                                pending.remove(&terminal_id).map(|entry| (result, entry));
                        },
                    }
                }
            }
            if generation_changed {
                retired_entry = pending.remove(&terminal_id);
            }
        }
        if let Some(entry) = retired_entry {
            retire_overlapped_write(entry, &self.retired_writes);
            return (0, Err(anyhow::Error::new(TerminalRunChangedBeforeWrite)));
        }
        if let Some(((written, result), entry)) = completed_entry {
            drop(entry);
            return (written, result.with_context(err_context));
        }

        // Duplicate under the lock, after the generation check, and write the
        // duplicate after releasing it. A pending overlapped write must not stall
        // every other pane's spawn, kill, or write. The duplicate still names
        // the console that passed the check.
        let (handle, incarnation) = {
            let terminals = match self.terminals.lock().to_anyhow().with_context(err_context) {
                Ok(terminals) => terminals,
                Err(error) => return (0, Err(error)),
            };
            match terminals.get(&terminal_id) {
                Some(Some(term)) => {
                    if let Some(expected) = generation {
                        if term.generation != Some(expected) {
                            return (0, Err(anyhow::Error::new(TerminalRunChangedBeforeWrite)));
                        }
                    }
                    match duplicate_write_handle(term.input_write_handle) {
                        Ok(handle) => (handle, term.incarnation),
                        Err(error) => return (0, Err(error).with_context(err_context)),
                    }
                },
                _ => {
                    return (
                        0,
                        Err(anyhow!("no ConPTY terminal found for id {}", terminal_id))
                            .with_context(err_context),
                    );
                },
            }
        };
        let (written, result, pending_entry) =
            start_overlapped_write(handle, buf, generation, incarnation);
        if let Some(entry) = pending_entry {
            // The pending operation now owns the duplicated handle.
            let replaced = self
                .pending_writes
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(terminal_id, entry);
            if let Some(replaced) = replaced {
                retire_overlapped_write(replaced, &self.retired_writes);
            }
            let still_current = self.terminals.lock().ok().and_then(|terminals| {
                terminals
                    .get(&terminal_id)
                    .and_then(|terminal| terminal.as_ref())
                    .map(|terminal| terminal.incarnation)
            }) == Some(incarnation);
            if !still_current {
                let stale = self
                    .pending_writes
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .remove(&terminal_id);
                if let Some(mut stale) = stale {
                    stale.cancel();
                    retire_overlapped_write(stale, &self.retired_writes);
                }
                return (
                    0,
                    Err(anyhow!("no ConPTY terminal found for id {}", terminal_id))
                        .with_context(err_context),
                );
            }
            return (0, Ok(()));
        }
        unsafe { CloseHandle(handle) };
        (written, result.with_context(err_context))
    }

    pub fn tcdrain(&self, terminal_id: u32) -> Result<()> {
        let err_context = || format!("failed to drain terminal {}", terminal_id);

        let handle = {
            let terminals = self
                .terminals
                .lock()
                .to_anyhow()
                .with_context(err_context)?;
            match terminals.get(&terminal_id) {
                Some(Some(term)) => duplicate_write_handle(term.input_write_handle)
                    .map_err(anyhow::Error::new)
                    .with_context(err_context)?,
                _ => {
                    return Err(anyhow!("no ConPTY terminal found for id {}", terminal_id))
                        .with_context(err_context);
                },
            }
        };
        let _close = CloseOnDrop(handle);
        let ok = unsafe { FlushFileBuffers(handle) };
        if ok == 0 {
            // FlushFileBuffers can legitimately fail on pipe handles
            // (ERROR_INVALID_FUNCTION) — treat as non-fatal.
            let e = io::Error::last_os_error();
            log::debug!("FlushFileBuffers on terminal {}: {}", terminal_id, e);
        }
        Ok(())
    }

    fn terminate_retained(&self, pid: u32) -> io::Result<()> {
        // Hold the map lock across TerminateProcess so a concurrent drop
        // cannot close the handle and let the numeric pid be reused first.
        let terminals = self.terminals.lock().unwrap_or_else(|e| e.into_inner());
        let Some(handle) = terminals
            .values()
            .flatten()
            .find(|term| term.child_pid == pid)
            .map(|term| term.process_handle)
        else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "no retained process handle for this pid; not signaling a recycled pid",
            ));
        };
        terminate_handle(handle)
    }

    pub fn kill(&self, pid: u32) -> Result<()> {
        self.terminate_retained(pid)
            .with_context(|| format!("failed to kill pid {}", pid))?;
        Ok(())
    }

    pub fn force_kill(&self, pid: u32) -> Result<()> {
        self.terminate_retained(pid)
            .with_context(|| format!("failed to force-kill pid {}", pid))?;
        Ok(())
    }

    pub fn send_sigint(&self, pid: u32) -> Result<()> {
        let err_context = || format!("failed to send SIGINT to pid {}", pid);
        let not_ours = || {
            Err(anyhow!(
                "no retained process handle for pid {pid}; not signaling a recycled pid"
            ))
            .with_context(err_context)
        };
        // Hold the map lock across the exit check and the ctrl event so the
        // handle cannot be closed, and the pid cannot be reused, in between.
        let terminals = self.terminals.lock().unwrap_or_else(|e| e.into_inner());
        let Some(handle) = terminals
            .values()
            .flatten()
            .find(|term| term.child_pid == pid)
            .map(|term| term.process_handle)
        else {
            return not_ours();
        };
        let mut code: u32 = 0;
        let got = unsafe { GetExitCodeProcess(handle, &mut code) };
        // A dead process keeps this handle, but Windows may already have
        // given its pid to someone else. Do not signal that number.
        if got == 0 || code != STILL_ACTIVE as u32 {
            return not_ours();
        }
        let ok = unsafe { GenerateConsoleCtrlEvent(CTRL_C_EVENT, pid) };
        if ok != 0 {
            return Ok(());
        }
        terminate_handle(handle).with_context(err_context)?;
        Ok(())
    }

    pub fn bind_terminal_generation(&self, terminal_id: u32, generation: u64) {
        let Ok(mut terminals) = self.terminals.lock() else {
            return;
        };
        if let Some(Some(term)) = terminals.get_mut(&terminal_id) {
            term.generation = Some(generation);
        }
    }

    pub fn reserve_terminal_id(&self, terminal_id: u32) {
        self.terminals.lock().unwrap().insert(terminal_id, None);
    }

    pub fn clear_terminal_id(&self, terminal_id: u32) {
        let removed_terminal = self.terminals.lock().unwrap().remove(&terminal_id);
        let pending_write = self
            .pending_writes
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&terminal_id);
        if let Some(mut pending_write) = pending_write {
            pending_write.cancel();
            retire_overlapped_write(pending_write, &self.retired_writes);
        }
        // ClosePseudoConsole may wait for output to be drained. Never hold the
        // terminal registry lock while that destructor runs, and revoke the
        // input write before tearing the console down.
        drop(removed_terminal);
        if let Ok(mut sizes) = self.last_terminal_sizes.lock() {
            sizes.remove(&terminal_id);
        }
    }
    pub fn cancel_pending_tty_write(&self, terminal_id: u32) {
        let pending_write = self
            .pending_writes
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&terminal_id);
        if let Some(mut pending_write) = pending_write {
            pending_write.cancel();
            retire_overlapped_write(pending_write, &self.retired_writes);
        }
    }

    pub fn next_terminal_id(&self) -> Option<u32> {
        Some(
            self.next_terminal_id_counter
                .fetch_add(1, Ordering::Relaxed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn cmd(exe: &str, args: &[&str]) -> RunCommand {
        RunCommand {
            command: PathBuf::from(exe),
            args: args.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn plain_arg_is_left_unquoted() {
        let s = build_command_line_string(&cmd("nvim", &["hello"]));
        assert_eq!(s, "\"nvim\" hello");
    }

    #[test]
    fn arg_with_space_is_quoted() {
        let s = build_command_line_string(&cmd("nvim", &["hello world"]));
        assert_eq!(s, "\"nvim\" \"hello world\"");
    }

    // Regression: zellij edit with a Windows backslash path plus a space in
    // the path used to eat every backslash inside the quoted arg. See
    // https://github.com/zellij-org/zellij/issues/5293.
    #[test]
    fn backslash_path_with_space_preserves_backslashes() {
        let path = r"C:\Users\me\Folder With Space\README.md";
        let s = build_command_line_string(&cmd("nvim", &[path]));
        assert_eq!(
            s,
            "\"nvim\" \"C:\\Users\\me\\Folder With Space\\README.md\""
        );
    }

    #[test]
    fn embedded_quote_is_escaped_with_odd_backslashes() {
        // Input arg: a"b (contains a literal quote). It should be emitted
        // as a\"b inside a quoted arg.
        let s = build_command_line_string(&cmd("prog", &["a\"b"]));
        assert_eq!(s, "\"prog\" \"a\\\"b\"");
    }

    #[test]
    fn backslash_before_embedded_quote_gets_doubled() {
        // Input arg: a\"b — one literal backslash, then a literal quote.
        // Must emit 3 backslashes then a quote so CommandLineToArgvW parses
        // back to a\"b.
        let s = build_command_line_string(&cmd("prog", &["a\\\"b c"]));
        assert_eq!(s, "\"prog\" \"a\\\\\\\"b c\"");
    }

    #[test]
    fn trailing_backslash_before_closing_quote_gets_doubled() {
        // "path with space\" would let the trailing backslash escape the
        // closing quote. The trailing backslash must be doubled.
        let s = build_command_line_string(&cmd("prog", &["C:\\my path\\"]));
        assert_eq!(s, "\"prog\" \"C:\\my path\\\\\"");
    }

    #[test]
    fn initial_conpty_size_defaults_to_nonzero_geometry() {
        let backend = WindowsPtyBackend::new().expect("backend");
        assert_eq!(backend.initial_size_for(1), (80, 24));
    }

    #[test]
    fn set_terminal_size_remembers_geometry_for_the_next_spawn() {
        let backend = WindowsPtyBackend::new().expect("backend");
        backend
            .set_terminal_size(4, 132, 43, None, None)
            .expect("remember size without an open ConPTY");
        assert_eq!(backend.initial_size_for(4), (132, 43));
        backend.clear_terminal_id(4);
        assert_eq!(backend.initial_size_for(4), (80, 24));
    }
}

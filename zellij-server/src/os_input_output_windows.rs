use crate::os_input_output::{command_exists, AsyncReader};
use crate::panes::PaneId;

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    io,
    os::windows::ffi::OsStrExt,
    os::windows::io::{FromRawHandle, IntoRawHandle, OwnedHandle},
    sync::{Arc, Mutex},
};

use tokio::io::AsyncReadExt;
use tokio::net::windows::named_pipe::NamedPipeServer;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, S_OK};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FlushFileBuffers, WriteFile, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{
    ClosePseudoConsole, CreatePseudoConsole, GenerateConsoleCtrlEvent, ResizePseudoConsole, COORD,
    CTRL_C_EVENT, HPCON,
};
use windows_sys::Win32::System::Pipes::{CreateNamedPipeW, CreatePipe};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
    InitializeProcThreadAttributeList, OpenProcess, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject, CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT, INFINITE,
    PROCESS_INFORMATION, PROCESS_TERMINATE, STARTUPINFOEXW, STARTUPINFOW,
};

use zellij_utils::{errors::prelude::*, input::command::RunCommand};

pub use async_trait::async_trait;

// Not exported by windows-sys; value from the Windows SDK.
const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x00020016;
const FILE_FLAG_FIRST_PIPE_INSTANCE: u32 = 0x00080000;
const PIPE_ACCESS_INBOUND: u32 = 0x00000001;
const PIPE_TYPE_BYTE: u32 = 0;
const PIPE_WAIT: u32 = 0;
const GENERIC_WRITE: u32 = 0x40000000;

/// Per-terminal ConPTY state.
struct ConPtyTerminal {
    hpcon: HPCON,
    input_write_handle: HANDLE,
}

// HANDLE/HPCON are isize (plain integers), safe to send across threads.
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
        }
    }
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
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        if let Some(handle) = self.pending.take() {
            let pipe =
                unsafe { NamedPipeServer::from_raw_handle(handle.into_raw_handle()) }?;
            self.pipe = Some(pipe);
        }
        let pipe = self
            .pipe
            .as_mut()
            .expect("ConPtyAsyncReader used after init");
        pipe.read(buf).await
    }
}

// ---------------------------------------------------------------------------
// Helper utilities
// ---------------------------------------------------------------------------

/// Encode a Rust string as null-terminated UTF-16.
fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// Build a Windows command-line string from a `RunCommand`, following the
/// `CommandLineToArgvW` quoting convention.
fn build_command_line(cmd: &RunCommand) -> Vec<u16> {
    let mut cmdline = String::new();

    // Executable — always quote to handle spaces in paths.
    let exe = cmd.command.to_string_lossy();
    cmdline.push('"');
    cmdline.push_str(&exe);
    cmdline.push('"');

    for arg in &cmd.args {
        cmdline.push(' ');
        if arg.is_empty() || arg.contains(' ') || arg.contains('\t') || arg.contains('"') {
            cmdline.push('"');
            let mut backslashes: usize = 0;
            for ch in arg.chars() {
                if ch == '\\' {
                    backslashes += 1;
                } else if ch == '"' {
                    // Double backslashes preceding a quote, then escape the quote.
                    for _ in 0..backslashes {
                        cmdline.push('\\');
                    }
                    backslashes = 0;
                    cmdline.push('\\');
                    cmdline.push('"');
                } else {
                    backslashes = 0;
                    cmdline.push(ch);
                }
            }
            // Double trailing backslashes before the closing quote.
            for _ in 0..backslashes {
                cmdline.push('\\');
            }
            cmdline.push('"');
        } else {
            cmdline.push_str(arg);
        }
    }

    to_wide(&cmdline)
}

/// Build a UTF-16 environment block (each entry `KEY=VALUE\0`, terminated by
/// an extra `\0`) from the current process environment, adding
/// `ZELLIJ_PANE_ID`.
fn build_environment_block(terminal_id: u32) -> Vec<u16> {
    let mut block: Vec<u16> = Vec::new();
    for (key, value) in std::env::vars() {
        if key == "ZELLIJ_PANE_ID" {
            continue;
        }
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
    let name = format!(
        r"\\.\pipe\zellij-pty-{}-{}",
        std::process::id(),
        terminal_id
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
            0,                    // no template
        )
    };
    if client == INVALID_HANDLE_VALUE {
        unsafe { CloseHandle(server) };
        return Err(io::Error::last_os_error());
    }

    Ok((server, client))
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
    si.lpAttributeList = attr_list;

    // --- command line & environment ---
    let mut cmd_line = build_command_line(cmd);
    let env_block = build_environment_block(terminal_id);

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
            std::ptr::null(),               // lpApplicationName
            cmd_line.as_mut_ptr(),           // lpCommandLine (mutable)
            std::ptr::null(),               // lpProcessAttributes
            std::ptr::null(),               // lpThreadAttributes
            0,                              // bInheritHandles = FALSE
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
            env_block.as_ptr().cast(),      // lpEnvironment
            cwd_ptr,                        // lpCurrentDirectory
            &si.StartupInfo as *const STARTUPINFOW, // lpStartupInfo
            &mut pi,                        // lpProcessInformation
        )
    };

    unsafe { DeleteProcThreadAttributeList(attr_list) };

    if ok == 0 {
        return Err(io::Error::last_os_error());
    }

    Ok((pi.hProcess, pi.hThread, pi.dwProcessId))
}

fn terminate_process(pid: u32) -> std::result::Result<(), std::io::Error> {
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if handle == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let ok = TerminateProcess(handle, 1);
        CloseHandle(handle);
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// WindowsPtyBackend
// ---------------------------------------------------------------------------

/// Windows PTY backend using native ConPTY with IOCP-based async I/O.
#[derive(Clone)]
pub(crate) struct WindowsPtyBackend {
    terminals: Arc<Mutex<BTreeMap<u32, Option<ConPtyTerminal>>>>,
}

impl WindowsPtyBackend {
    pub fn new() -> Result<Self, io::Error> {
        Ok(Self {
            terminals: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    /// Core spawn logic — creates ConPTY, spawns child, sets up exit monitor.
    fn do_spawn(
        &self,
        cmd: RunCommand,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        terminal_id: u32,
    ) -> Result<(Box<dyn AsyncReader>, u32)> {
        let err_context = |c: &RunCommand| {
            format!(
                "failed to spawn terminal for '{}'",
                c.command.to_string_lossy()
            )
        };

        // 1. Output pipe pair (named, overlapped read end for IOCP)
        let (output_read, output_write) = create_overlapped_output_pipe(terminal_id)
            .with_context(|| err_context(&cmd))?;

        // 2. Input pipe pair (anonymous, both synchronous)
        let mut input_read: HANDLE = 0;
        let mut input_write: HANDLE = 0;
        if unsafe { CreatePipe(&mut input_read, &mut input_write, std::ptr::null(), 0) } == 0 {
            unsafe {
                CloseHandle(output_read);
                CloseHandle(output_write);
            }
            return Err(io::Error::last_os_error())
                .with_context(|| err_context(&cmd));
        }

        // 3. Create pseudo console
        let hpcon = match create_conpty(80, 24, input_read, output_write) {
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
            match spawn_child_process(hpcon, &cmd, terminal_id) {
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

        // 6. Store per-terminal state
        self.terminals.lock().unwrap().insert(
            terminal_id,
            Some(ConPtyTerminal {
                hpcon,
                input_write_handle: input_write,
            }),
        );

        // 7. Exit-monitoring thread (zero CPU — spends all time in kernel wait)
        let cmd_for_monitor = cmd.clone();
        std::thread::spawn(move || {
            let exit_code = unsafe {
                WaitForSingleObject(process_handle, INFINITE);
                let mut code: u32 = 0;
                GetExitCodeProcess(process_handle, &mut code);
                CloseHandle(process_handle);
                code
            };
            quit_cb(
                PaneId::Terminal(terminal_id),
                Some(exit_code as i32),
                cmd_for_monitor,
            );
        });

        // 8. Wrap the output read handle in an async reader
        let owned =
            unsafe { OwnedHandle::from_raw_handle(output_read as *mut core::ffi::c_void) };
        let reader = Box::new(ConPtyAsyncReader::new(owned)) as Box<dyn AsyncReader>;

        Ok((reader, child_pid))
    }

    pub fn spawn_terminal(
        &self,
        cmd: RunCommand,
        failover_cmd: Option<RunCommand>,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        terminal_id: u32,
    ) -> Result<(Box<dyn AsyncReader>, u32)> {
        if command_exists(&cmd) {
            return self.do_spawn(cmd, quit_cb, terminal_id);
        }
        if let Some(failover) = failover_cmd {
            if command_exists(&failover) {
                return self.do_spawn(failover, quit_cb, terminal_id);
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
                    }
                }
            },
            _ => {
                Err::<(), _>(anyhow!(
                    "no ConPTY terminal found for id {}",
                    terminal_id
                ))
                .with_context(err_context)
                .non_fatal();
            },
        }
        Ok(())
    }

    pub fn write_to_tty_stdin(&self, terminal_id: u32, buf: &[u8]) -> Result<usize> {
        let err_context = || format!("failed to write to stdin of terminal {}", terminal_id);

        match self
            .terminals
            .lock()
            .to_anyhow()
            .with_context(err_context)?
            .get(&terminal_id)
        {
            Some(Some(term)) => {
                let mut written: u32 = 0;
                let ok = unsafe {
                    WriteFile(
                        term.input_write_handle,
                        buf.as_ptr(),
                        buf.len() as u32,
                        &mut written,
                        std::ptr::null_mut(),
                    )
                };
                if ok == 0 {
                    Err(io::Error::last_os_error()).with_context(err_context)
                } else {
                    Ok(written as usize)
                }
            },
            _ => Err(anyhow!("no ConPTY terminal found for id {}", terminal_id))
                .with_context(err_context),
        }
    }

    pub fn tcdrain(&self, terminal_id: u32) -> Result<()> {
        let err_context = || format!("failed to drain terminal {}", terminal_id);

        match self
            .terminals
            .lock()
            .to_anyhow()
            .with_context(err_context)?
            .get(&terminal_id)
        {
            Some(Some(term)) => {
                let ok = unsafe { FlushFileBuffers(term.input_write_handle) };
                if ok == 0 {
                    // FlushFileBuffers can legitimately fail on pipe handles
                    // (ERROR_INVALID_FUNCTION) — treat as non-fatal.
                    let e = io::Error::last_os_error();
                    log::debug!("FlushFileBuffers on terminal {}: {}", terminal_id, e);
                }
                Ok(())
            },
            _ => Err(anyhow!("no ConPTY terminal found for id {}", terminal_id))
                .with_context(err_context),
        }
    }

    pub fn kill(&self, pid: u32) -> Result<()> {
        terminate_process(pid).with_context(|| format!("failed to kill pid {}", pid))?;
        Ok(())
    }

    pub fn force_kill(&self, pid: u32) -> Result<()> {
        terminate_process(pid).with_context(|| format!("failed to force-kill pid {}", pid))?;
        Ok(())
    }

    pub fn send_sigint(&self, pid: u32) -> Result<()> {
        let ok = unsafe { GenerateConsoleCtrlEvent(CTRL_C_EVENT, pid) };
        if ok != 0 {
            Ok(())
        } else {
            terminate_process(pid)
                .with_context(|| format!("failed to send SIGINT to pid {}", pid))?;
            Ok(())
        }
    }

    pub fn reserve_terminal_id(&self, terminal_id: u32) {
        self.terminals.lock().unwrap().insert(terminal_id, None);
    }

    pub fn clear_terminal_id(&self, terminal_id: u32) {
        self.terminals.lock().unwrap().remove(&terminal_id);
    }

    pub fn next_terminal_id(&self) -> Option<u32> {
        self.terminals
            .lock()
            .unwrap()
            .keys()
            .copied()
            .collect::<BTreeSet<u32>>()
            .last()
            .map(|l| l + 1)
            .or(Some(0))
    }
}

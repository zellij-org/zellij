pub const BUFSIZE: DWORD = 512;

use std::{
    ffi::{OsStr, OsString},
    io,
    mem::MaybeUninit,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, IntoRawHandle, OwnedHandle},
    },
    path::PathBuf,
    ptr,
    sync::Arc,
};

use winapi::{
    shared::{
        minwindef::{DWORD, TRUE},
        winerror::{ERROR_IO_PENDING, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED},
    },
    um::{
        errhandlingapi::GetLastError,
        fileapi::{CreateFileW, FlushFileBuffers, ReadFile, WriteFile, OPEN_EXISTING},
        handleapi::{CloseHandle, DuplicateHandle, INVALID_HANDLE_VALUE},
        ioapiset::GetOverlappedResult,
        minwinbase::OVERLAPPED,
        namedpipeapi::{ConnectNamedPipe, CreateNamedPipeW},
        processthreadsapi::GetCurrentProcess,
        synchapi::CreateEventW,
        winbase::{
            FILE_FLAG_OVERLAPPED, PIPE_ACCESS_DUPLEX, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES,
            PIPE_WAIT,
        },
        winnt::{FILE_SHARE_READ, FILE_SHARE_WRITE, GENERIC_READ, GENERIC_WRITE, HANDLE},
    },
};

macro_rules! call_BOOL_with_last_error {
    ($call: expr) => {
        if ($call) != 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    };
}
macro_rules! call_with_last_error {
    ($call: expr) => {{
        let value = $call;
        if value != INVALID_HANDLE_VALUE {
            Ok(value)
        } else {
            Err(std::io::Error::last_os_error())
        }
    }};
}

#[repr(transparent)]
struct EventedOverlapped(OVERLAPPED);

impl Drop for EventedOverlapped {
    fn drop(&mut self) {
        if (!self.0.hEvent.is_null()) {
            unsafe { CloseHandle(self.0.hEvent); }
        }
    }
}

/// Helper function to create an instance of [OVERLAPPED] with a new unique event
fn create_overlapped_with_new_event() -> io::Result<EventedOverlapped> {
    let mut overlapped = create_zeroed_overlapped();
    overlapped.hEvent = {
        let value = unsafe { CreateEventW(ptr::null_mut(), TRUE, TRUE, ptr::null_mut()) };
        if !value.is_null() {
            Ok(value)
        } else {
            Err(std::io::Error::last_os_error())
        }
    }?;

    Ok(EventedOverlapped(overlapped))
}

/// Helper function to create an zeroed instance of [OVERLAPPED]
fn create_zeroed_overlapped() -> OVERLAPPED {
    // SAFETY: Docs state to use an OVERLAPPED-Struct with all Members zeroed
    unsafe { MaybeUninit::zeroed().assume_init() }
}

#[derive(Debug, PartialEq, Clone)]
pub struct Pipe {
    pipe_name: Arc<[u16]>,
}

struct PipeAcceptIterator {
    pipe: Pipe,
}

impl Pipe {
    pub fn new(name: impl AsRef<OsStr>) -> Self {
        let pipe_name = Pipe::convert_pipe_name(name.as_ref());
        Self {
            pipe_name: Arc::from(pipe_name),
        }
    }

    pub fn incoming(&self) -> impl Iterator<Item = io::Result<PipeStream>> {
        PipeAcceptIterator { pipe: self.clone() }
    }

    pub fn connect(&self) -> io::Result<PipeStream> {
        loop {
            let client: Result<HANDLE, io::Error> = call_with_last_error!(unsafe {
                CreateFileW(
                    self.pipe_name.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    ptr::null_mut(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED,
                    ptr::null_mut(),
                )
            });

            match client {
                Ok(handle) => break Ok(handle.into()),
                Err(err) if err.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                    continue;
                },
                // TODO check when a call to WaitNamedPipe is usefull
                Err(err) => return Err(err),
            };
        }
    }

    pub fn accept(&self) -> io::Result<PipeStream> {
        let server_listener_pipe_handle = unsafe {
            CreateNamedPipeW(
                self.pipe_name.as_ptr(),                   // pipe name
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED, // read/write access
                PIPE_TYPE_BYTE |       // message type pipe 
                    PIPE_WAIT, // blocking mode
                PIPE_UNLIMITED_INSTANCES,                  // max. instances
                BUFSIZE,                                   // output buffer size
                BUFSIZE,                                   // input buffer size
                0,                                         // client time-out
                ptr::null_mut(),
            ) // default security attribute
        };

        if server_listener_pipe_handle == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }

        // Wait for the client to connect; if it succeeds,
        // the function returns a nonzero value. If the function
        // returns zero, GetLastError returns ERROR_PIPE_CONNECTED.

        let mut pipe_handle = PipeStream::from(server_listener_pipe_handle);
        let connected = if unsafe {
            ConnectNamedPipe(pipe_handle.0.as_raw_handle() as _, ptr::null_mut())
        } != 0
        {
            Ok(())
        } else {
            let os_error_code = unsafe { GetLastError() };
            if ERROR_PIPE_CONNECTED == os_error_code {
                Ok(())
            } else {
                let os_error = io::Error::from_raw_os_error(os_error_code as i32);

                Err(os_error)
            }
        };

        connected.map(|_| pipe_handle)
    }

    fn convert_pipe_name(name: &OsStr) -> Vec<u16> {
        let mut pipe_name = OsString::from("\\\\.\\pipe\\");
        pipe_name.push(name);
        let mut pipe_name = pipe_name.as_os_str().encode_wide().collect::<Vec<_>>();
        pipe_name.push(0);

        pipe_name
    }
}

impl Iterator for PipeAcceptIterator {
    type Item = io::Result<PipeStream>;

    fn next(&mut self) -> Option<Self::Item> {
        // TODO check for errors that require an abort of the server
        Some(self.pipe.accept())
    }
}

#[derive(Debug)]
/// Wraps a Handle to a pipe
/// This differs from []
pub struct PipeStream(OwnedHandle);

impl PipeStream {
    /// Tries to create a new Handle from `self` using [`DuplicateHandle`](https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-duplicatehandle)
    pub fn try_clone(&self) -> io::Result<Self> {
        self.try_clone_impl(unsafe { GetCurrentProcess() }, unsafe {
            GetCurrentProcess()
        })
    }

    /// Tries to creat a new Handle from `self` to send it to the specified process
    pub fn try_clone_for_process(
        &self,
        other: std::process::Child,
    ) -> Result<Self, std::io::Error> {
        self.try_clone_impl(unsafe { GetCurrentProcess() }, other.as_raw_handle() as _)
    }

    /// Tries to create a new Handle using [`DuplicateHandle`](https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-duplicatehandle)
    fn try_clone_impl(&self, source_process: HANDLE, target_process: HANDLE) -> io::Result<Self> {
        let mut dup_handle: HANDLE = ptr::null_mut();
        call_BOOL_with_last_error!(unsafe {
            DuplicateHandle(
                source_process,
                self.0.as_raw_handle() as _,
                target_process,
                (&mut dup_handle) as _,
                GENERIC_READ | GENERIC_WRITE,
                0,
                0,
            )
        })
        .map(|_| Self::from(dup_handle))
    }
}

impl IntoRawHandle for PipeStream {
    fn into_raw_handle(self) -> std::os::windows::prelude::RawHandle {
        self.0.into_raw_handle()
    }
}

impl From<HANDLE> for PipeStream {
    fn from(value: HANDLE) -> Self {
        let handle = unsafe { OwnedHandle::from_raw_handle(value as _) };
        Self(handle)
    }
}

impl std::io::Read for PipeStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut overlapped = create_overlapped_with_new_event()?;
        let mut consumed = 0;
        let result = call_BOOL_with_last_error!(unsafe {
            ReadFile(
                self.0.as_raw_handle() as _,
                buf.as_mut_ptr() as _,
                buf.len()
                    .clamp(u32::MIN as usize, usize::max(usize::MAX, u32::MAX as usize))
                    as u32,
                &mut consumed,
                &mut overlapped.0,
            )
        });
        match result {
            Ok(()) => Ok(consumed as usize),
            Err(err) if err.raw_os_error() == Some(ERROR_IO_PENDING as i32) => {
                call_BOOL_with_last_error!(unsafe {
                    GetOverlappedResult(
                        self.0.as_raw_handle() as _,
                        &mut overlapped.0 as *mut OVERLAPPED,
                        &mut consumed,
                        TRUE.into(),
                    )
                })
                .map(|_| consumed as usize)
            },
            Err(err) => Err(err),
        }
    }
}

impl std::io::Write for PipeStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut overlapped = create_overlapped_with_new_event()?;
        let mut consumed = 0;
        let result = call_BOOL_with_last_error!(unsafe {
            WriteFile(
                self.0.as_raw_handle() as _,
                buf.as_ptr() as _,
                buf.len()
                    .clamp(u32::MIN as usize, usize::max(usize::MAX, u32::MAX as usize))
                    as u32,
                &mut consumed,
                &mut overlapped.0 as *mut OVERLAPPED,
            )
        });
        match result {
            Ok(()) => Ok(consumed as usize),
            Err(err) if err.raw_os_error() == Some(ERROR_IO_PENDING as i32) => {
                call_BOOL_with_last_error!(unsafe {
                    GetOverlappedResult(
                        self.0.as_raw_handle() as _,
                        &mut overlapped.0 as *mut OVERLAPPED,
                        &mut consumed,
                        TRUE.into(),
                    )
                })
                .map(|_| consumed as usize)
            },
            Err(err) => Err(err),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        call_BOOL_with_last_error!(unsafe { FlushFileBuffers(self.0.as_raw_handle() as _) })
    }
}

// SAFETY: Microsoft sample does send a HANDLE from one thread to another.
// You even can send a handle from one process to another using DuplicateHandle
unsafe impl Send for PipeStream {}

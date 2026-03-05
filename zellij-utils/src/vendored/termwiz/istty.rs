//! Making it a little more convenient and safe to query whether
//! something is a terminal teletype or not.
//! This module defines the IsTty trait and the is_tty method to
//! return true if the item represents a terminal.
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
#[cfg(windows)]
use winapi::um::consoleapi::GetConsoleMode;

/// Adds the is_tty method to types that might represent a terminal
pub trait IsTty {
    /// Returns true if the instance is a terminal teletype, false
    /// otherwise.
    fn is_tty(&self) -> bool;
}

/// On unix, the `isatty()` library function returns true if a file
/// descriptor is a terminal.  Let's implement `IsTty` for anything
/// that has an associated raw file descriptor.
#[cfg(unix)]
impl<S: AsRawFd> IsTty for S {
    fn is_tty(&self) -> bool {
        let fd = self.as_raw_fd();
        unsafe { libc::isatty(fd) == 1 }
    }
}

#[cfg(windows)]
impl<S: AsRawHandle> IsTty for S {
    fn is_tty(&self) -> bool {
        let mut mode = 0;
        let ok = unsafe { GetConsoleMode(self.as_raw_handle() as *mut _, &mut mode) };
        ok == 1
    }
}

//! An abstraction over a terminal device

use crate::vendored::termwiz::caps::probed::ProbeCapabilities;
use crate::vendored::termwiz::caps::Capabilities;
use crate::vendored::termwiz::input::InputEvent;
use crate::vendored::termwiz::surface::Change;
use crate::vendored::termwiz::Result;
use crate::vendored_termwiz_format_err as format_err;
use num_traits::NumCast;
use std::fmt::Display;
use std::time::Duration;

#[cfg(unix)]
pub mod unix;
#[cfg(windows)]
pub mod windows;

pub mod buffered;

#[cfg(unix)]
pub use self::unix::{UnixTerminal, UnixTerminalWaker as TerminalWaker};
#[cfg(windows)]
pub use self::windows::{WindowsTerminal, WindowsTerminalWaker as TerminalWaker};

/// Represents the size of the terminal screen.
/// The number of rows and columns of character cells are expressed.
/// Some implementations populate the size of those cells in pixels.
// On Windows, GetConsoleFontSize() can return the size of a cell in
// logical units and we can probably use this to populate xpixel, ypixel.
// GetConsoleScreenBufferInfo() can return the rows and cols.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenSize {
    /// The number of rows of text
    pub rows: usize,
    /// The number of columns per row
    pub cols: usize,
    /// The width of a cell in pixels.  Some implementations never
    /// set this to anything other than zero.
    pub xpixel: usize,
    /// The height of a cell in pixels.  Some implementations never
    /// set this to anything other than zero.
    pub ypixel: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocking {
    DoNotWait,
    Wait,
}

/// `Terminal` abstracts over some basic terminal capabilities.
/// If the `set_raw_mode` or `set_cooked_mode` functions are used in
/// any combination, the implementation is required to restore the
/// terminal mode that was in effect when it was created.
pub trait Terminal {
    /// Raw mode disables input line buffering, allowing data to be
    /// read as the user presses keys, disables local echo, so keys
    /// pressed by the user do not implicitly render to the terminal
    /// output, and disables canonicalization of unix newlines to CRLF.
    fn set_raw_mode(&mut self) -> Result<()>;
    fn set_cooked_mode(&mut self) -> Result<()>;

    /// Enter the alternate screen.  The alternate screen will be left
    /// automatically when the `Terminal` is dropped.
    fn enter_alternate_screen(&mut self) -> Result<()>;

    /// Exit the alternate screen.
    fn exit_alternate_screen(&mut self) -> Result<()>;

    /// Queries the current screen size, returning width, height.
    fn get_screen_size(&mut self) -> Result<ScreenSize>;

    /// Returns a capability probing helper that will use escape
    /// sequences to attempt to probe information from the terminal
    fn probe_capabilities(&mut self) -> Option<ProbeCapabilities> {
        None
    }

    /// Sets the current screen size
    fn set_screen_size(&mut self, size: ScreenSize) -> Result<()>;

    /// Render a series of changes to the terminal output
    fn render(&mut self, changes: &[Change]) -> Result<()>;

    /// Flush any buffered output
    fn flush(&mut self) -> Result<()>;

    /// Check for a parsed input event.
    /// `wait` indicates the behavior in the case that no input is
    /// immediately available.  If wait is `None` then `poll_input`
    /// will not return until an event is available.  If wait is
    /// `Some(duration)` then `poll_input` will wait up to the given
    /// duration for an event before returning with a value of
    /// `Ok(None)`.  If wait is `Some(Duration::ZERO)` then the
    /// poll is non-blocking.
    ///
    /// The possible values returned as `InputEvent`s depend on the
    /// mode of the terminal.  Most values are not returned unless
    /// the terminal is set to raw mode.
    fn poll_input(&mut self, wait: Option<Duration>) -> Result<Option<InputEvent>>;

    fn waker(&self) -> TerminalWaker;
}

/// `SystemTerminal` is a concrete implementation of `Terminal`.
/// Ideally you wouldn't reference `SystemTerminal` in consuming
/// code.  This type is exposed for convenience if you are doing
/// something unusual and want easier access to the constructors.
#[cfg(unix)]
pub type SystemTerminal = UnixTerminal;
#[cfg(windows)]
pub type SystemTerminal = WindowsTerminal;

/// Construct a new instance of Terminal.
/// The terminal will have a renderer that is influenced by the configuration
/// in the provided `Capabilities` instance.
/// The terminal will explicitly open `/dev/tty` on Unix systems and
/// `CONIN$` and `CONOUT$` on Windows systems, so that it should yield a
/// functioning console with minimal headaches.
/// If you have a more advanced use case you will want to look to the
/// constructors for `UnixTerminal` and `WindowsTerminal` and call whichever
/// one is most suitable for your needs.
pub fn new_terminal(caps: Capabilities) -> Result<impl Terminal> {
    SystemTerminal::new(caps)
}

pub(crate) fn cast<T: NumCast + Display + Copy, U: NumCast>(n: T) -> Result<U> {
    num_traits::cast(n).ok_or_else(|| format_err!("{} is out of bounds for this system", n))
}

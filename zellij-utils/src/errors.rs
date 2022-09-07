//! Error context system based on a thread-local representation of the call stack, itself based on
//! the instructions that are sent between threads.
//!
//! # Help wanted
//!
//! As of writing this, zellij relies on `unwrap()` to catch errors (terminate execution) in many
//! functions, rather than returning a [`Result`] to propagate these errors further up. While we
//! don't consider `unwrap` to be a bad thing in general, it hides the underlying error and leaves
//! the user only with a stack trace to go on. Worse than this, it will crash the application. This
//! is particularly bad when the user is using long-running sessions to perform tasks.
//!
//! Hence, we would like to eliminate `unwrap()` statements from the code where possible, and apply
//! better error handling instead. This way, functions higher up in the call stack can react to
//! errors from underlying functions and either try to recover, or give some meaningful error
//! messages if recovery isn't possible.
//!
//! Since the zellij codebase is pretty big and growing rapidly, this endeavour will continue to be
//! pursued over time, as zellij develops. The idea is that modules or single files are converted
//! bit by bit, preferrably in small PRs that each target a specific module or file. **If you are
//! looking to contribute to zellij, this may be an ideal start for you!** This way you get to know
//! the codebase and get an idea which modules are used at which other places in the code.
//!
//! If you have an interest in this, don't hesitate to get in touch with us.
//!
//!
//! # Error handling facilities
//!
//! ## Displaying panic messages
//!
//! Panics are generally handled via the [`Panic`] error type and the
//! [`handle_panic`][`handle_panic`] panic handler function. The fancy formatting is performed by
//! the [`miette`] crate.
//!
//!
//! ## Propagating errors
//!
//! We use the [`anyhow`] crate to propagate errors up the call stack. At the moment, zellij
//! doesn't have custom error types, so we wrap whatever errors the underlying libraries give us,
//! if any. [`anyhow`] serves the purpose of providing [`context`][`context`] about where (i.e.
//! under which circumstances) an error happened.
//!
//! A critical requirement for propagating errors is that all functions involved must return the
//! [`Result`] type. This allows convenient error handling with the `?` operator.
//!
//! At some point you will likely stop propagating errors and decide what to do with the error.
//! Generally you can:
//!
//! 1. Try to recover from the error, or
//! 2. Report the error to the user and either
//!     1. Terminate program execution (See [`fatal`][`fatal`]), or
//!     2. Continue program execution (See [`non_fatal`][`non_fatal`])
//!
//!
//! ## Handling errors
//!
//! Ideally, when the program encounters an error it will try to recover as best as it can. This
//! can mean falling back to some sane default if a specific value (e.g. an environment variable)
//! cannot be found. Note that this isn't always applicable. If in doubt, don't hesitate to ask.
//!
//! Recovery usually isn't an option if an operation has changed the internal state (i.e. the value
//! or content of specific variables) of objects in the code. In this case, if an error is
//! encountered, it is best to declare the program state corrupted and terminate the whole
//! application. This can be done by [`unwrap`]ing on the [`Result`] type. Always try to propagate
//! the error as best as you can and attach meaningful context before [`unwrap`]ing. This gives the
//! user an idea what went wrong and can also help developers in quickly identifying which parts of
//! the code to debug if necessary.
//!
//! When you encounter such a fatal error and cannot propagate it further up (e.g. because the
//! current function cannot be changed to return a [`Result`], or because it is the "root" function
//! of a program thread), use the [`fatal`][`fatal`] function to panic the application. It will
//! attach some small context to the error and finally [`unwrap`] it. Using this function over the
//! regular [`unwrap`] has the added benefit that other developers seeing this in the code know
//! that someone has previously spent some thought about error handling at this location.
//!
//! If you encounter a non-fatal error, use the [`non_fatal`][`non_fatal`] function to handle
//! it. Instead of [`panic`]ing the application, the error is written to the application log and
//! execution continues. Please use this sparingly, as an error usually calls for actions to be
//! taken rather than ignoring it.
//!
//!
//! [`handle_panic`]: not_wasm::handle_panic
//! [`context`]: anyhow::Context
//! [`fatal`]: FatalError::fatal
//! [`non_fatal`]: FatalError::non_fatal

use anyhow::Context;
use colored::*;
use log::error;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Error, Formatter};

use miette::Diagnostic;
use thiserror::Error as ThisError;

/// Re-exports of common error-handling code.
pub mod prelude {
    pub use super::FatalError;
    pub use super::LoggableError;
    pub use anyhow::anyhow;
    pub use anyhow::bail;
    pub use anyhow::Context;
    pub use anyhow::Result;
}

pub trait ErrorInstruction {
    fn error(err: String) -> Self;
}

#[derive(Debug, ThisError, Diagnostic)]
#[error("{0}{}", self.show_backtrace())]
#[diagnostic(help("{}", self.show_help()))]
struct Panic(String);

impl Panic {
    fn show_backtrace(&self) -> String {
        if let Ok(var) = std::env::var("RUST_BACKTRACE") {
            if !var.is_empty() && var != "0" {
                return format!("\n{:?}", backtrace::Backtrace::new());
            }
        }
        "".into()
    }

    fn show_help(&self) -> String {
        r#"If you are seeing this message, it means that something went wrong.
Please report this error to the github issue.
(https://github.com/zellij-org/zellij/issues)

Also, if you want to see the backtrace, you can set the `RUST_BACKTRACE` environment variable to `1`.
"#.into()
    }
}

/// Helper trait to easily log error types.
///
/// The `print_error` function takes a closure which takes a `&str` and fares with it as necessary
/// to log the error to some usable location. For convenience, logging to stdout, stderr and
/// `log::error!` is already implemented.
///
/// Note that the trait functions pass the error through unmodified, so they can be chained with
/// the usual handling of [`std::result::Result`] types.
pub trait LoggableError<T>: Sized {
    /// Gives a formatted error message derived from `self` to the closure `fun` for
    /// printing/logging as appropriate.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// use anyhow;
    /// use zellij_utils::errors::LoggableError;
    ///
    /// let my_err: anyhow::Result<&str> = Err(anyhow::anyhow!("Test error"));
    /// my_err
    ///     .print_error(|msg| println!("{msg}"))
    ///     .unwrap();
    /// ```
    fn print_error<F: Fn(&str)>(self, fun: F) -> Self;

    /// Convenienve function, calls `print_error` with the closure `|msg| log::error!("{}", msg)`.
    fn to_log(self) -> Self {
        self.print_error(|msg| log::error!("{}", msg))
    }

    /// Convenienve function, calls `print_error` with the closure `|msg| eprintln!("{}", msg)`.
    fn to_stderr(self) -> Self {
        self.print_error(|msg| eprintln!("{}", msg))
    }

    /// Convenienve function, calls `print_error` with the closure `|msg| println!("{}", msg)`.
    fn to_stdout(self) -> Self {
        self.print_error(|msg| println!("{}", msg))
    }
}

impl<T> LoggableError<T> for anyhow::Result<T> {
    fn print_error<F: Fn(&str)>(self, fun: F) -> Self {
        if let Err(ref err) = self {
            let mut msg = format!("ERROR: {}", err);
            for cause in err.chain().skip(1) {
                msg = format!("{msg}\nbecause: {cause}");
            }
            fun(&msg);
        }
        self
    }
}

/// Special trait to mark fatal/non-fatal errors.
///
/// This works in tandem with `LoggableError` above and is meant to make reading code easier with
/// regard to whether an error is fatal or not (i.e. can be ignored, or at least doesn't make the
/// application crash).
///
/// This essentially degrades any `std::result::Result<(), _>` to a simple `()`.
pub trait FatalError<T> {
    /// Mark results as being non-fatal.
    ///
    /// If the result is an `Err` variant, this will [print the error to the log][`to_log`].
    /// Discards the result type afterwards.
    ///
    /// [`to_log`]: LoggableError::to_log
    fn non_fatal(self);

    /// Mark results as being fatal.
    ///
    /// If the result is an `Err` variant, this will unwrap the error and panic the application.
    /// If the result is an `Ok` variant, the inner value is unwrapped and returned instead.
    ///
    /// # Panics
    ///
    /// If the given result is an `Err` variant.
    #[track_caller]
    fn fatal(self) -> T;
}

/// Helper function to silence `#[warn(unused_must_use)]` cargo warnings. Used exclusively in
/// `FatalError::non_fatal`!
fn discard_result<T>(_arg: anyhow::Result<T>) {}

impl<T> FatalError<T> for anyhow::Result<T> {
    fn non_fatal(self) {
        if self.is_err() {
            discard_result(self.context("a non-fatal error occured").to_log());
        }
    }

    fn fatal(self) -> T {
        if let Ok(val) = self {
            val
        } else {
            self.context("a fatal error occured")
                .expect("Program terminates")
        }
    }
}

/// Different types of calls that form an [`ErrorContext`] call stack.
///
/// Complex variants store a variant of a related enum, whose variants can be built from
/// the corresponding Zellij MSPC instruction enum variants ([`ScreenInstruction`],
/// [`PtyInstruction`], [`ClientInstruction`], etc).
#[derive(Copy, Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum ContextType {
    /// A screen-related call.
    Screen(ScreenContext),
    /// A PTY-related call.
    Pty(PtyContext),
    /// A plugin-related call.
    Plugin(PluginContext),
    /// An app-related call.
    Client(ClientContext),
    /// A server-related call.
    IPCServer(ServerContext),
    StdinHandler,
    AsyncTask,
    PtyWrite(PtyWriteContext),
    /// An empty, placeholder call. This should be thought of as representing no call at all.
    /// A call stack representation filled with these is the representation of an empty call stack.
    Empty,
}

impl Display for ContextType {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        if let Some((left, right)) = match *self {
            ContextType::Screen(c) => Some(("screen_thread:", format!("{:?}", c))),
            ContextType::Pty(c) => Some(("pty_thread:", format!("{:?}", c))),
            ContextType::Plugin(c) => Some(("plugin_thread:", format!("{:?}", c))),
            ContextType::Client(c) => Some(("main_thread:", format!("{:?}", c))),
            ContextType::IPCServer(c) => Some(("ipc_server:", format!("{:?}", c))),
            ContextType::StdinHandler => Some(("stdin_handler_thread:", "AcceptInput".to_string())),
            ContextType::AsyncTask => Some(("stream_terminal_bytes:", "AsyncTask".to_string())),
            ContextType::PtyWrite(c) => Some(("pty_writer_thread:", format!("{:?}", c))),
            ContextType::Empty => None,
        } {
            write!(f, "{} {}", left.purple(), right.green())
        } else {
            write!(f, "")
        }
    }
}

// FIXME: Just deriving EnumDiscriminants from strum will remove the need for any of this!!!
/// Stack call representations corresponding to the different types of [`ScreenInstruction`]s.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ScreenContext {
    HandlePtyBytes,
    Render,
    NewPane,
    OpenInPlaceEditor,
    ToggleFloatingPanes,
    TogglePaneEmbedOrFloating,
    HorizontalSplit,
    VerticalSplit,
    WriteCharacter,
    ResizeLeft,
    ResizeRight,
    ResizeDown,
    ResizeUp,
    ResizeIncrease,
    ResizeDecrease,
    SwitchFocus,
    FocusNextPane,
    FocusPreviousPane,
    FocusPaneAt,
    MoveFocusLeft,
    MoveFocusLeftOrPreviousTab,
    MoveFocusDown,
    MoveFocusUp,
    MoveFocusRight,
    MoveFocusRightOrNextTab,
    MovePane,
    MovePaneDown,
    MovePaneUp,
    MovePaneRight,
    MovePaneLeft,
    Exit,
    DumpScreen,
    EditScrollback,
    ScrollUp,
    ScrollUpAt,
    ScrollDown,
    ScrollDownAt,
    ScrollToBottom,
    PageScrollUp,
    PageScrollDown,
    HalfPageScrollUp,
    HalfPageScrollDown,
    ClearScroll,
    CloseFocusedPane,
    ToggleActiveSyncTab,
    ToggleActiveTerminalFullscreen,
    TogglePaneFrames,
    SetSelectable,
    SetInvisibleBorders,
    SetFixedHeight,
    SetFixedWidth,
    ClosePane,
    UpdatePaneName,
    UndoRenamePane,
    NewTab,
    SwitchTabNext,
    SwitchTabPrev,
    CloseTab,
    GoToTab,
    UpdateTabName,
    UndoRenameTab,
    TerminalResize,
    TerminalPixelDimensions,
    TerminalBackgroundColor,
    TerminalForegroundColor,
    TerminalColorRegisters,
    ChangeMode,
    LeftClick,
    RightClick,
    MiddleClick,
    LeftMouseRelease,
    RightMouseRelease,
    MiddleMouseRelease,
    MouseHoldLeft,
    MouseHoldRight,
    MouseHoldMiddle,
    Copy,
    ToggleTab,
    AddClient,
    RemoveClient,
    AddOverlay,
    RemoveOverlay,
    ConfirmPrompt,
    DenyPrompt,
    UpdateSearch,
    SearchDown,
    SearchUp,
    SearchToggleCaseSensitivity,
    SearchToggleWholeWord,
    SearchToggleWrap,
}

/// Stack call representations corresponding to the different types of [`PtyInstruction`]s.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PtyContext {
    SpawnTerminal,
    OpenInPlaceEditor,
    SpawnTerminalVertically,
    SpawnTerminalHorizontally,
    UpdateActivePane,
    GoToTab,
    NewTab,
    ClosePane,
    CloseTab,
    Exit,
}

/// Stack call representations corresponding to the different types of [`PluginInstruction`]s.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PluginContext {
    Load,
    Update,
    Render,
    Unload,
    Exit,
    AddClient,
    RemoveClient,
}

/// Stack call representations corresponding to the different types of [`ClientInstruction`]s.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ClientContext {
    Exit,
    Error,
    UnblockInputThread,
    Render,
    ServerError,
    SwitchToMode,
    Connected,
    ActiveClients,
}

/// Stack call representations corresponding to the different types of [`ServerInstruction`]s.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ServerContext {
    NewClient,
    Render,
    UnblockInputThread,
    ClientExit,
    RemoveClient,
    Error,
    KillSession,
    DetachSession,
    AttachClient,
    ConnStatus,
    ActiveClients,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PtyWriteContext {
    Write,
    Exit,
}

#[cfg(not(target_family = "wasm"))]
pub use not_wasm::*;

#[cfg(not(target_family = "wasm"))]
mod not_wasm {
    use super::*;
    use crate::channels::{SenderWithContext, ASYNCOPENCALLS, OPENCALLS};
    use miette::{GraphicalReportHandler, GraphicalTheme, Report};
    use std::panic::PanicInfo;

    /// The maximum amount of calls an [`ErrorContext`] will keep track
    /// of in its stack representation. This is a per-thread maximum.
    const MAX_THREAD_CALL_STACK: usize = 6;

    /// Custom panic handler/hook. Prints the [`ErrorContext`].
    pub fn handle_panic<T>(info: &PanicInfo<'_>, sender: &SenderWithContext<T>)
    where
        T: ErrorInstruction + Clone,
    {
        use std::{process, thread};
        let thread = thread::current();
        let thread = thread.name().unwrap_or("unnamed");

        let msg = match info.payload().downcast_ref::<&'static str>() {
            Some(s) => Some(*s),
            None => info.payload().downcast_ref::<String>().map(|s| &**s),
        }
        .unwrap_or("An unexpected error occurred!");

        let err_ctx = OPENCALLS.with(|ctx| *ctx.borrow());

        let mut report: Report = Panic(format!("\u{1b}[0;31m{}\u{1b}[0;0m", msg)).into();

        let mut location_string = String::new();
        if let Some(location) = info.location() {
            location_string = format!(
                "At {}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            );
            report = report.wrap_err(location_string.clone());
        }

        if !err_ctx.is_empty() {
            report = report.wrap_err(format!("{}", err_ctx));
        }

        report = report.wrap_err(format!(
            "Thread '\u{1b}[0;31m{}\u{1b}[0;0m' panicked.",
            thread
        ));

        error!(
            "{}",
            format!(
                "Panic occured:
             thread: {}
             location: {}
             message: {}",
                thread, location_string, msg
            )
        );

        if thread == "main" {
            // here we only show the first line because the backtrace is not readable otherwise
            // a better solution would be to escape raw mode before we do this, but it's not trivial
            // to get os_input here
            println!("\u{1b}[2J{}", fmt_report(report));
            process::exit(1);
        } else {
            let _ = sender.send(T::error(fmt_report(report)));
        }
    }

    pub fn get_current_ctx() -> ErrorContext {
        ASYNCOPENCALLS
            .try_with(|ctx| *ctx.borrow())
            .unwrap_or_else(|_| OPENCALLS.with(|ctx| *ctx.borrow()))
    }

    fn fmt_report(diag: Report) -> String {
        let mut out = String::new();
        GraphicalReportHandler::new_themed(GraphicalTheme::unicode())
            .render_report(&mut out, diag.as_ref())
            .unwrap();
        out
    }

    /// A representation of the call stack.
    #[derive(Clone, Copy, Serialize, Deserialize, Debug)]
    pub struct ErrorContext {
        calls: [ContextType; MAX_THREAD_CALL_STACK],
    }

    impl ErrorContext {
        /// Returns a new, blank [`ErrorContext`] containing only [`Empty`](ContextType::Empty)
        /// calls.
        pub fn new() -> Self {
            Self {
                calls: [ContextType::Empty; MAX_THREAD_CALL_STACK],
            }
        }

        /// Returns `true` if the calls has all [`Empty`](ContextType::Empty) calls.
        pub fn is_empty(&self) -> bool {
            self.calls.iter().all(|c| c == &ContextType::Empty)
        }

        /// Adds a call to this [`ErrorContext`]'s call stack representation.
        pub fn add_call(&mut self, call: ContextType) {
            for ctx in &mut self.calls {
                if let ContextType::Empty = ctx {
                    *ctx = call;
                    break;
                }
            }
            self.update_thread_ctx()
        }

        /// Updates the thread local [`ErrorContext`].
        pub fn update_thread_ctx(&self) {
            ASYNCOPENCALLS
                .try_with(|ctx| *ctx.borrow_mut() = *self)
                .unwrap_or_else(|_| OPENCALLS.with(|ctx| *ctx.borrow_mut() = *self));
        }
    }

    impl Default for ErrorContext {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Display for ErrorContext {
        fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
            writeln!(f, "Originating Thread(s)")?;
            for (index, ctx) in self.calls.iter().enumerate() {
                if *ctx == ContextType::Empty {
                    break;
                }
                writeln!(f, "\t\u{1b}[0;0m{}. {}", index + 1, ctx)?;
            }
            Ok(())
        }
    }
}

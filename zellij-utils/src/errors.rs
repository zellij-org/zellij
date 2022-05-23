//! Error context system based on a thread-local representation of the call stack, itself based on
//! the instructions that are sent between threads.

use crate::channels::{SenderWithContext, ASYNCOPENCALLS, OPENCALLS};
use colored::*;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Error, Formatter};
use std::panic::PanicInfo;

use miette::{Diagnostic, GraphicalReportHandler, GraphicalTheme, Report};
use thiserror::Error as ThisError;

/// The maximum amount of calls an [`ErrorContext`] will keep track
/// of in its stack representation. This is a per-thread maximum.
const MAX_THREAD_CALL_STACK: usize = 6;

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

fn fmt_report(diag: Report) -> String {
    let mut out = String::new();
    GraphicalReportHandler::new_themed(GraphicalTheme::unicode())
        .render_report(&mut out, diag.as_ref())
        .unwrap();
    out
}

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

    if let Some(location) = info.location() {
        report = report.wrap_err(format!(
            "At {}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        ));
    }

    if !err_ctx.is_empty() {
        report = report.wrap_err(format!("{}", err_ctx));
    }

    report = report.wrap_err(format!(
        "Thread '\u{1b}[0;31m{}\u{1b}[0;0m' panicked.",
        thread
    ));

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
    NewTab,
    SwitchTabNext,
    SwitchTabPrev,
    CloseTab,
    GoToTab,
    UpdateTabName,
    TerminalResize,
    TerminalPixelDimensions,
    TerminalBackgroundColor,
    TerminalForegroundColor,
    TerminalColorRegister,
    ChangeMode,
    LeftClick,
    RightClick,
    MouseRelease,
    MouseHold,
    Copy,
    ToggleTab,
    AddClient,
    RemoveClient,
    AddOverlay,
    RemoveOverlay,
    ConfirmPrompt,
    DenyPrompt,
}

/// Stack call representations corresponding to the different types of [`PtyInstruction`]s.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PtyContext {
    SpawnTerminal,
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
}

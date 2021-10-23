//! Error context system based on a thread-local representation of the call stack, itself based on
//! the instructions that are sent between threads.

use crate::channels::{SenderWithContext, ASYNCOPENCALLS, OPENCALLS};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Error, Formatter};
use std::panic::PanicInfo;

/// The maximum amount of calls an [`ErrorContext`] will keep track
/// of in its stack representation. This is a per-thread maximum.
const MAX_THREAD_CALL_STACK: usize = 6;

pub trait ErrorInstruction {
    fn error(err: String) -> Self;
}

/// Custom panic handler/hook. Prints the [`ErrorContext`].
pub fn handle_panic<T>(info: &PanicInfo<'_>, sender: &SenderWithContext<T>)
where
    T: ErrorInstruction + Clone,
{
    use backtrace::Backtrace;
    use std::{process, thread};
    let backtrace = Backtrace::new();
    let thread = thread::current();
    let thread = thread.name().unwrap_or("unnamed");

    let msg = match info.payload().downcast_ref::<&'static str>() {
        Some(s) => Some(*s),
        None => info.payload().downcast_ref::<String>().map(|s| &**s),
    };

    let err_ctx = OPENCALLS.with(|ctx| *ctx.borrow());

    let backtrace = match (info.location(), msg) {
        (Some(location), Some(msg)) => format!(
            "{}\n\u{1b}[0;0mError: \u{1b}[0;31mthread '{}' panicked at '{}': {}:{}\n\u{1b}[0;0m{:?}",
            err_ctx,
            thread,
            msg,
            location.file(),
            location.line(),
            backtrace,
        ),
        (Some(location), None) => format!(
            "{}\n\u{1b}[0;0mError: \u{1b}[0;31mthread '{}' panicked: {}:{}\n\u{1b}[0;0m{:?}",
            err_ctx,
            thread,
            location.file(),
            location.line(),
            backtrace
        ),
        (None, Some(msg)) => format!(
            "{}\n\u{1b}[0;0mError: \u{1b}[0;31mthread '{}' panicked at '{}'\n\u{1b}[0;0m{:?}",
            err_ctx, thread, msg, backtrace
        ),
        (None, None) => format!(
            "{}\n\u{1b}[0;0mError: \u{1b}[0;31mthread '{}' panicked\n\u{1b}[0;0m{:?}",
            err_ctx, thread, backtrace
        ),
    };

    let one_line_backtrace = match (info.location(), msg) {
        (Some(location), Some(msg)) => format!(
            "{}\n\u{1b}[0;0mError: \u{1b}[0;31mthread '{}' panicked at '{}': {}:{}\n\u{1b}[0;0m",
            err_ctx,
            thread,
            msg,
            location.file(),
            location.line(),
        ),
        (Some(location), None) => format!(
            "{}\n\u{1b}[0;0mError: \u{1b}[0;31mthread '{}' panicked: {}:{}\n\u{1b}[0;0m",
            err_ctx,
            thread,
            location.file(),
            location.line(),
        ),
        (None, Some(msg)) => format!(
            "{}\n\u{1b}[0;0mError: \u{1b}[0;31mthread '{}' panicked at '{}'\n\u{1b}[0;0m",
            err_ctx, thread, msg
        ),
        (None, None) => format!(
            "{}\n\u{1b}[0;0mError: \u{1b}[0;31mthread '{}' panicked\n\u{1b}[0;0m",
            err_ctx, thread
        ),
    };

    if thread == "main" {
        // here we only show the first line because the backtrace is not readable otherwise
        // a better solution would be to escape raw mode before we do this, but it's not trivial
        // to get os_input here
        println!("\u{1b}[2J{}", one_line_backtrace);
        process::exit(1);
    } else {
        let _ = sender.send(T::error(backtrace));
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

    /// Adds a call to this [`ErrorContext`]'s call stack representation.
    pub fn add_call(&mut self, call: ContextType) {
        for ctx in self.calls.iter_mut() {
            if *ctx == ContextType::Empty {
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
        writeln!(f, "Originating Thread(s):")?;
        for (index, ctx) in self.calls.iter().enumerate() {
            if *ctx == ContextType::Empty {
                break;
            }
            writeln!(f, "\u{1b}[0;0m{}. {}", index + 1, ctx)?;
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

// TODO use the `colored` crate for color formatting
impl Display for ContextType {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        let purple = "\u{1b}[1;35m";
        let green = "\u{1b}[0;32m";
        match *self {
            ContextType::Screen(c) => write!(f, "{}screen_thread: {}{:?}", purple, green, c),
            ContextType::Pty(c) => write!(f, "{}pty_thread: {}{:?}", purple, green, c),
            ContextType::Plugin(c) => write!(f, "{}plugin_thread: {}{:?}", purple, green, c),
            ContextType::Client(c) => write!(f, "{}main_thread: {}{:?}", purple, green, c),
            ContextType::IPCServer(c) => write!(f, "{}ipc_server: {}{:?}", purple, green, c),
            ContextType::StdinHandler => {
                write!(f, "{}stdin_handler_thread: {}AcceptInput", purple, green)
            }
            ContextType::AsyncTask => {
                write!(f, "{}stream_terminal_bytes: {}AsyncTask", purple, green)
            }
            ContextType::Empty => write!(f, ""),
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
    NewTab,
    SwitchTabNext,
    SwitchTabPrev,
    CloseTab,
    GoToTab,
    UpdateTabName,
    TerminalResize,
    ChangeMode,
    LeftClick,
    MouseRelease,
    MouseHold,
    Copy,
    ToggleTab,
    AddClient,
    RemoveClient,
}

/// Stack call representations corresponding to the different types of [`PtyInstruction`]s.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PtyContext {
    SpawnTerminal,
    SpawnTerminalVertically,
    SpawnTerminalHorizontally,
    UpdateActivePane,
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
}

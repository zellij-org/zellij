//! Error context system based on a thread-local representation of the call stack, itself based on
//! the instructions that are sent between threads.

use serde::{Deserialize, Serialize};

use std::fmt::{Display, Error, Formatter};

use crate::client::ClientInstruction;
use crate::common::thread_bus::{ASYNCOPENCALLS, OPENCALLS};
use crate::pty::PtyInstruction;
use crate::screen::ScreenInstruction;
use crate::server::ServerInstruction;

/// The maximum amount of calls an [`ErrorContext`] will keep track
/// of in its stack representation. This is a per-thread maximum.
const MAX_THREAD_CALL_STACK: usize = 6;

#[cfg(not(test))]
use super::thread_bus::SenderWithContext;
#[cfg(not(test))]
use std::panic::PanicInfo;
/// Custom panic handler/hook. Prints the [`ErrorContext`].
#[cfg(not(test))]
pub fn handle_panic(
    info: &PanicInfo<'_>,
    send_app_instructions: &SenderWithContext<ClientInstruction>,
) {
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

    if thread == "main" {
        println!("{}", backtrace);
        process::exit(1);
    } else {
        let _ = send_app_instructions.send(ClientInstruction::Error(backtrace));
    }
}

pub fn get_current_ctx() -> ErrorContext {
    ASYNCOPENCALLS
        .try_with(|ctx| *ctx.borrow())
        .unwrap_or_else(|_| OPENCALLS.with(|ctx| *ctx.borrow()))
}

/// A representation of the call stack.
#[derive(Clone, Copy, Serialize, Deserialize)]
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
#[derive(Copy, Clone, PartialEq, Serialize, Deserialize)]
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
    SwitchFocus,
    FocusNextPane,
    FocusPreviousPane,
    MoveFocusLeft,
    MoveFocusLeftOrPreviousTab,
    MoveFocusDown,
    MoveFocusUp,
    MoveFocusRight,
    MoveFocusRightOrNextTab,
    Exit,
    ScrollUp,
    ScrollDown,
    PageScrollUp,
    PageScrollDown,
    ClearScroll,
    CloseFocusedPane,
    ToggleActiveSyncTab,
    ToggleActiveTerminalFullscreen,
    SetSelectable,
    SetInvisibleBorders,
    SetMaxHeight,
    ClosePane,
    ApplyLayout,
    NewTab,
    SwitchTabNext,
    SwitchTabPrev,
    CloseTab,
    GoToTab,
    UpdateTabName,
    TerminalResize,
    ChangeMode,
}

// FIXME: Just deriving EnumDiscriminants from strum will remove the need for any of this!!!
impl From<&ScreenInstruction> for ScreenContext {
    fn from(screen_instruction: &ScreenInstruction) -> Self {
        match *screen_instruction {
            ScreenInstruction::PtyBytes(..) => ScreenContext::HandlePtyBytes,
            ScreenInstruction::Render => ScreenContext::Render,
            ScreenInstruction::NewPane(_) => ScreenContext::NewPane,
            ScreenInstruction::HorizontalSplit(_) => ScreenContext::HorizontalSplit,
            ScreenInstruction::VerticalSplit(_) => ScreenContext::VerticalSplit,
            ScreenInstruction::WriteCharacter(_) => ScreenContext::WriteCharacter,
            ScreenInstruction::ResizeLeft => ScreenContext::ResizeLeft,
            ScreenInstruction::ResizeRight => ScreenContext::ResizeRight,
            ScreenInstruction::ResizeDown => ScreenContext::ResizeDown,
            ScreenInstruction::ResizeUp => ScreenContext::ResizeUp,
            ScreenInstruction::SwitchFocus => ScreenContext::SwitchFocus,
            ScreenInstruction::FocusNextPane => ScreenContext::FocusNextPane,
            ScreenInstruction::FocusPreviousPane => ScreenContext::FocusPreviousPane,
            ScreenInstruction::MoveFocusLeft => ScreenContext::MoveFocusLeft,
            ScreenInstruction::MoveFocusLeftOrPreviousTab => {
                ScreenContext::MoveFocusLeftOrPreviousTab
            }
            ScreenInstruction::MoveFocusDown => ScreenContext::MoveFocusDown,
            ScreenInstruction::MoveFocusUp => ScreenContext::MoveFocusUp,
            ScreenInstruction::MoveFocusRight => ScreenContext::MoveFocusRight,
            ScreenInstruction::MoveFocusRightOrNextTab => ScreenContext::MoveFocusRightOrNextTab,
            ScreenInstruction::Exit => ScreenContext::Exit,
            ScreenInstruction::ScrollUp => ScreenContext::ScrollUp,
            ScreenInstruction::ScrollDown => ScreenContext::ScrollDown,
            ScreenInstruction::PageScrollUp => ScreenContext::PageScrollUp,
            ScreenInstruction::PageScrollDown => ScreenContext::PageScrollDown,
            ScreenInstruction::ClearScroll => ScreenContext::ClearScroll,
            ScreenInstruction::CloseFocusedPane => ScreenContext::CloseFocusedPane,
            ScreenInstruction::ToggleActiveTerminalFullscreen => {
                ScreenContext::ToggleActiveTerminalFullscreen
            }
            ScreenInstruction::SetSelectable(..) => ScreenContext::SetSelectable,
            ScreenInstruction::SetInvisibleBorders(..) => ScreenContext::SetInvisibleBorders,
            ScreenInstruction::SetMaxHeight(..) => ScreenContext::SetMaxHeight,
            ScreenInstruction::ClosePane(_) => ScreenContext::ClosePane,
            ScreenInstruction::ApplyLayout(..) => ScreenContext::ApplyLayout,
            ScreenInstruction::NewTab(_) => ScreenContext::NewTab,
            ScreenInstruction::SwitchTabNext => ScreenContext::SwitchTabNext,
            ScreenInstruction::SwitchTabPrev => ScreenContext::SwitchTabPrev,
            ScreenInstruction::CloseTab => ScreenContext::CloseTab,
            ScreenInstruction::GoToTab(_) => ScreenContext::GoToTab,
            ScreenInstruction::UpdateTabName(_) => ScreenContext::UpdateTabName,
            ScreenInstruction::TerminalResize(_) => ScreenContext::TerminalResize,
            ScreenInstruction::ChangeMode(_) => ScreenContext::ChangeMode,
            ScreenInstruction::ToggleActiveSyncTab => ScreenContext::ToggleActiveSyncTab,
        }
    }
}

/// Stack call representations corresponding to the different types of [`PtyInstruction`]s.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PtyContext {
    SpawnTerminal,
    SpawnTerminalVertically,
    SpawnTerminalHorizontally,
    NewTab,
    ClosePane,
    CloseTab,
    Exit,
}

impl From<&PtyInstruction> for PtyContext {
    fn from(pty_instruction: &PtyInstruction) -> Self {
        match *pty_instruction {
            PtyInstruction::SpawnTerminal(_) => PtyContext::SpawnTerminal,
            PtyInstruction::SpawnTerminalVertically(_) => PtyContext::SpawnTerminalVertically,
            PtyInstruction::SpawnTerminalHorizontally(_) => PtyContext::SpawnTerminalHorizontally,
            PtyInstruction::ClosePane(_) => PtyContext::ClosePane,
            PtyInstruction::CloseTab(_) => PtyContext::CloseTab,
            PtyInstruction::NewTab => PtyContext::NewTab,
            PtyInstruction::Exit => PtyContext::Exit,
        }
    }
}

// FIXME: This whole pattern *needs* a macro eventually, it's soul-crushing to write

use crate::wasm_vm::PluginInstruction;

/// Stack call representations corresponding to the different types of [`PluginInstruction`]s.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PluginContext {
    Load,
    Update,
    Render,
    Unload,
    Exit,
}

impl From<&PluginInstruction> for PluginContext {
    fn from(plugin_instruction: &PluginInstruction) -> Self {
        match *plugin_instruction {
            PluginInstruction::Load(..) => PluginContext::Load,
            PluginInstruction::Update(..) => PluginContext::Update,
            PluginInstruction::Render(..) => PluginContext::Render,
            PluginInstruction::Unload(_) => PluginContext::Unload,
            PluginInstruction::Exit => PluginContext::Exit,
        }
    }
}

/// Stack call representations corresponding to the different types of [`ClientInstruction`]s.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ClientContext {
    Exit,
    Error,
    UnblockInputThread,
    Render,
}

impl From<&ClientInstruction> for ClientContext {
    fn from(client_instruction: &ClientInstruction) -> Self {
        match *client_instruction {
            ClientInstruction::Exit => ClientContext::Exit,
            ClientInstruction::Error(_) => ClientContext::Error,
            ClientInstruction::Render(_) => ClientContext::Render,
            ClientInstruction::UnblockInputThread => ClientContext::UnblockInputThread,
        }
    }
}

/// Stack call representations corresponding to the different types of [`ServerInstruction`]s.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ServerContext {
    NewClient,
    Action,
    Render,
    TerminalResize,
    UnblockInputThread,
    ClientExit,
}

impl From<&ServerInstruction> for ServerContext {
    fn from(server_instruction: &ServerInstruction) -> Self {
        match *server_instruction {
            ServerInstruction::NewClient(..) => ServerContext::NewClient,
            ServerInstruction::Action(_) => ServerContext::Action,
            ServerInstruction::TerminalResize(_) => ServerContext::TerminalResize,
            ServerInstruction::Render(_) => ServerContext::Render,
            ServerInstruction::UnblockInputThread => ServerContext::UnblockInputThread,
            ServerInstruction::ClientExit => ServerContext::ClientExit,
        }
    }
}

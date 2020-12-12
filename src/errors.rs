use crate::pty_bus::PtyInstruction;
use crate::screen::ScreenInstruction;
use crate::{AppInstruction, SenderWithContext, OPENCALLS};
use backtrace::Backtrace;
use std::fmt::{Display, Error, Formatter};
use std::panic::PanicInfo;
use std::{process, thread};

const MAX_THREAD_CALL_STACK: usize = 6;

pub fn handle_panic(
    info: &PanicInfo<'_>,
    send_app_instructions: &SenderWithContext<AppInstruction>,
) {
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
            backtrace
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
        send_app_instructions
            .send(AppInstruction::Error(backtrace))
            .unwrap();
    }
}

#[derive(Clone, Copy)]
pub struct ErrorContext {
    calls: [ContextType; MAX_THREAD_CALL_STACK],
}

impl ErrorContext {
    pub fn new() -> Self {
        Self {
            calls: [ContextType::Empty; MAX_THREAD_CALL_STACK],
        }
    }

    pub fn add_call(&mut self, call: ContextType) {
        for ctx in self.calls.iter_mut() {
            if *ctx == ContextType::Empty {
                *ctx = call;
                break;
            }
        }
        OPENCALLS.with(|ctx| *ctx.borrow_mut() = *self);
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

#[derive(Copy, Clone, PartialEq)]
pub enum ContextType {
    Screen(ScreenContext),
    Pty(PtyContext),
    App(AppContext),
    IPCServer,
    StdinHandler,
    AsyncTask,
    Empty,
}

impl Display for ContextType {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        let purple = "\u{1b}[1;35m";
        let green = "\u{1b}[0;32m";
        match *self {
            ContextType::Screen(c) => write!(f, "{}screen_thread: {}{:?}", purple, green, c),
            ContextType::Pty(c) => write!(f, "{}pty_thread: {}{:?}", purple, green, c),
            ContextType::App(c) => write!(f, "{}main_thread: {}{:?}", purple, green, c),
            ContextType::IPCServer => write!(f, "{}ipc_server: {}AcceptInput", purple, green),
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScreenContext {
    HandlePtyEvent,
    Render,
    NewPane,
    HorizontalSplit,
    VerticalSplit,
    WriteCharacter,
    ResizeLeft,
    ResizeRight,
    ResizeDown,
    ResizeUp,
    MoveFocus,
    MoveFocusLeft,
    MoveFocusDown,
    MoveFocusUp,
    MoveFocusRight,
    Quit,
    ScrollUp,
    ScrollDown,
    ClearScroll,
    CloseFocusedPane,
    ToggleActiveTerminalFullscreen,
    ClosePane,
    ApplyLayout,
    NewTab,
    SwitchTabNext,
    SwitchTabPrev,
}

impl From<&ScreenInstruction> for ScreenContext {
    fn from(screen_instruction: &ScreenInstruction) -> Self {
        match *screen_instruction {
            ScreenInstruction::Pty(..) => ScreenContext::HandlePtyEvent,
            ScreenInstruction::Render => ScreenContext::Render,
            ScreenInstruction::NewPane(_) => ScreenContext::NewPane,
            ScreenInstruction::HorizontalSplit(_) => ScreenContext::HorizontalSplit,
            ScreenInstruction::VerticalSplit(_) => ScreenContext::VerticalSplit,
            ScreenInstruction::WriteCharacter(_) => ScreenContext::WriteCharacter,
            ScreenInstruction::ResizeLeft => ScreenContext::ResizeLeft,
            ScreenInstruction::ResizeRight => ScreenContext::ResizeRight,
            ScreenInstruction::ResizeDown => ScreenContext::ResizeDown,
            ScreenInstruction::ResizeUp => ScreenContext::ResizeUp,
            ScreenInstruction::MoveFocus => ScreenContext::MoveFocus,
            ScreenInstruction::MoveFocusLeft => ScreenContext::MoveFocusLeft,
            ScreenInstruction::MoveFocusDown => ScreenContext::MoveFocusDown,
            ScreenInstruction::MoveFocusUp => ScreenContext::MoveFocusUp,
            ScreenInstruction::MoveFocusRight => ScreenContext::MoveFocusRight,
            ScreenInstruction::Quit => ScreenContext::Quit,
            ScreenInstruction::ScrollUp => ScreenContext::ScrollUp,
            ScreenInstruction::ScrollDown => ScreenContext::ScrollDown,
            ScreenInstruction::ClearScroll => ScreenContext::ClearScroll,
            ScreenInstruction::CloseFocusedPane => ScreenContext::CloseFocusedPane,
            ScreenInstruction::ToggleActiveTerminalFullscreen => {
                ScreenContext::ToggleActiveTerminalFullscreen
            }
            ScreenInstruction::ClosePane(_) => ScreenContext::ClosePane,
            ScreenInstruction::ApplyLayout(_) => ScreenContext::ApplyLayout,
            ScreenInstruction::NewTab => ScreenContext::NewTab,
            ScreenInstruction::SwitchTabNext => ScreenContext::SwitchTabNext,
            ScreenInstruction::SwitchTabPrev => ScreenContext::SwitchTabPrev,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PtyContext {
    SpawnTerminal,
    SpawnTerminalVertically,
    SpawnTerminalHorizontally,
    ClosePane,
    Quit,
}

impl From<&PtyInstruction> for PtyContext {
    fn from(pty_instruction: &PtyInstruction) -> Self {
        match *pty_instruction {
            PtyInstruction::SpawnTerminal(_) => PtyContext::SpawnTerminal,
            PtyInstruction::SpawnTerminalVertically(_) => PtyContext::SpawnTerminalVertically,
            PtyInstruction::SpawnTerminalHorizontally(_) => PtyContext::SpawnTerminalHorizontally,
            PtyInstruction::ClosePane(_) => PtyContext::ClosePane,
            PtyInstruction::Quit => PtyContext::Quit,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppContext {
    Exit,
    Error,
}

impl From<&AppInstruction> for AppContext {
    fn from(app_instruction: &AppInstruction) -> Self {
        match *app_instruction {
            AppInstruction::Exit => AppContext::Exit,
            AppInstruction::Error(_) => AppContext::Error,
        }
    }
}

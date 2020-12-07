use crate::pty_bus::PtyInstruction;
use crate::screen::ScreenInstruction;
use crate::{AppInstruction, OPENCALLS};
use backtrace::Backtrace;
use std::panic::PanicInfo;
use std::sync::mpsc::SyncSender;
use std::{process, thread};

pub fn handle_panic(
    info: &PanicInfo<'_>,
    send_app_instructions: &SyncSender<(AppInstruction, ErrorContext)>,
) {
    let backtrace = Backtrace::new();
    let thread = thread::current();
    let thread = thread.name().unwrap_or("unnamed");

    let msg = match info.payload().downcast_ref::<&'static str>() {
        Some(s) => Some(*s),
        None => info.payload().downcast_ref::<String>().map(|s| &**s),
    };

    let mut err_ctx: ErrorContext = OPENCALLS.with(|ctx| ctx.borrow().clone());

    let backtrace = match (info.location(), msg) {
        (Some(location), Some(msg)) => format!(
            "thread '{}' panicked at '{}': {}:{}\n{:#?}\n{:?}",
            thread,
            msg,
            location.file(),
            location.line(),
            err_ctx,
            backtrace
        ),
        (Some(location), None) => format!(
            "thread '{}' panicked: {}:{}\n{:#?}\n{:?}",
            thread,
            location.file(),
            location.line(),
            err_ctx,
            backtrace
        ),
        (None, Some(msg)) => format!(
            "thread '{}' panicked at '{}'\n{:#?}\n{:?}",
            thread, msg, err_ctx, backtrace
        ),
        (None, None) => format!(
            "thread '{}' panicked\n{:#?}\n{:?}",
            thread, err_ctx, backtrace
        ),
    };

    if thread == "main" {
        println!("{}", backtrace);
        process::exit(1);
    } else {
        let instruction = AppInstruction::Error(backtrace);
        err_ctx.add_call(ContextType::App(AppContext::from(&instruction)));
        send_app_instructions.send((instruction, err_ctx)).unwrap();
    }
}

#[derive(Clone, Debug)]
pub struct ErrorContext {
    calls: Vec<ContextType>,
}

impl ErrorContext {
    pub fn new() -> Self {
        Self { calls: Vec::new() }
    }

    pub fn add_call(&mut self, call: ContextType) {
        self.calls.push(call);
        OPENCALLS.with(|ctx| *ctx.borrow_mut() = self.clone());
    }
}

#[derive(Debug, Copy, Clone)]
pub enum ContextType {
    Screen(ScreenContext),
    Pty(PtyContext),
    App(AppContext),
    IPCServer,
    StdinHandler,
    AsyncTask,
}

#[derive(Debug, Clone, Copy)]
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
        }
    }
}

#[derive(Debug, Clone, Copy)]
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

#[derive(Debug, Clone, Copy)]
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

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
        err_ctx.add_call("panic_hook(handle_panic)");
        send_app_instructions
            .send((AppInstruction::Error(backtrace), err_ctx))
            .unwrap();
    }
}

#[derive(Clone, Debug)]
pub struct ErrorContext {
    calls: Vec<String>,
}

impl ErrorContext {
    pub fn new() -> Self {
        Self { calls: Vec::new() }
    }

    pub fn add_call(&mut self, call: &str) {
        self.calls.push(call.into());
        OPENCALLS.with(|ctx| *ctx.borrow_mut() = self.clone());
    }
}

pub trait InstType {}

impl InstType for AppInstruction {}

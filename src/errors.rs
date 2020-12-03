use crate::AppInstruction;
use backtrace::Backtrace;
use std::panic::PanicInfo;
use std::sync::mpsc::SyncSender;
use std::{process, thread};

pub fn handle_panic(info: &PanicInfo<'_>, send_app_instructions: &SyncSender<AppInstruction>) {
    let backtrace = Backtrace::new();
    let thread = thread::current();
    let thread = thread.name().unwrap_or("unnamed");

    let msg = match info.payload().downcast_ref::<&'static str>() {
        Some(s) => Some(*s),
        None => info.payload().downcast_ref::<String>().map(|s| &**s),
    };

    let backtrace = match (info.location(), msg) {
        (Some(location), Some(msg)) => {
            format!(
                "\nthread '{}' panicked at '{}': {}:{}\n{:?}",
                thread,
                msg,
                location.file(),
                location.line(),
                backtrace
            )
        }
        (Some(location), None) => {
            format!(
                "\nthread '{}' panicked: {}:{}\n{:?}",
                thread,
                location.file(),
                location.line(),
                backtrace
            )
        }
        (None, Some(msg)) => {
            format!(
                "\nthread '{}' panicked at '{}'\n{:?}",
                thread, msg, backtrace
            )
        }
        (None, None) => {
            format!("\nthread '{}' panicked\n{:?}", thread, backtrace)
        }
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

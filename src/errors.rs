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
        Some(s) => *s,
        None => match info.payload().downcast_ref::<String>() {
            Some(s) => &**s,
            None => "Box<Any>",
        },
    };

    let backtrace = match info.location() {
        Some(location) => {
            format!(
                "\nthread '{}' panicked at '{}': {}:{}\n{:?}",
                thread,
                msg,
                location.file(),
                location.line(),
                backtrace
            )
        }
        None => {
            format!(
                "\nthread '{}' panicked at '{}'\n{:?}",
                thread, msg, backtrace
            )
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

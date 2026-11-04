#[macro_use]
mod diagnostics;
mod app;
mod config;
mod emulation;
pub mod icns;
mod input;
mod launch;
mod platform;
#[cfg_attr(not(feature = "web_server_capability"), path = "remote_disabled.rs")]
mod remote;
mod render;
mod session;
#[cfg(test)]
mod testing;
mod text;
mod window;
#[cfg(windows)]
mod windows_shell;

use anyhow::{bail, Result};
use zellij_utils::cli::{CliArgs, TerminalArgs};

pub use app::probe::{NotificationProbe, ProbeEvent};
pub use app::{clear_all_sessions, detach_everywhere, stop_all_sessions};

#[cfg(all(target_os = "linux", feature = "test-support"))]
pub mod test_support {
    pub use crate::app::desktop::dbus::fake::*;
    pub use crate::app::desktop::dbus::{Kind, Message, Value};
}

pub fn run(args: TerminalArgs, opts: CliArgs) -> Result<()> {
    diagnostics::log_crashes();
    config::identity::declare();
    if let Err(e) = args.validate() {
        bail!("{}", e);
    }
    let run = platform::os::Run::prepare();
    session::spawn::forget_launching_session();
    if args.waits_for_command() {
        let code = launch::run_waited(args, opts)?;
        std::process::exit(code);
    }
    let result = if args.background {
        launch::run_in_background(args, opts)
    } else {
        launch::open(args, opts)
    };
    run.finish(&result);
    result
}

#[cfg(windows)]
pub fn unregister_app() -> Result<Vec<String>> {
    windows_shell::app_registration::unregister()
}

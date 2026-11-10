use anyhow::Result;

use crate::window::AppPoster;
use crate::windows_shell::unattended::{self, Unattended};

pub struct Run(Option<Unattended>);

impl Run {
    pub fn prepare() -> Self {
        Run(unattended::prepare())
    }

    pub fn finish(&self, result: &Result<()>) {
        if let (Some(unattended), Err(error)) = (&self.0, result) {
            unattended.report(error);
        }
    }
}

pub fn claim_app_identity() {
    if let Err(e) = crate::windows_shell::app_registration::register() {
        report!(
            "the app could not register its name and icon for desktop notifications: {:#}",
            e
        );
    }
    crate::windows_shell::jump_list::install();
}

pub fn terminate_on_signal(poster: AppPoster) {
    crate::windows_shell::console_events::end_on_console_event(move || {
        poster.post(crate::app::AppEvent::Terminate);
    });
}

pub fn detach_from_terminal(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

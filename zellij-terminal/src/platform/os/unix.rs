use anyhow::Result;

use crate::window::AppPoster;

pub struct Run;

impl Run {
    pub fn prepare() -> Self {
        Run
    }

    pub fn finish(&self, _result: &Result<()>) {}
}

pub fn claim_app_identity() {}

pub fn terminate_on_signal(poster: AppPoster) {
    use signal_hook::consts::{SIGINT, SIGTERM};
    use signal_hook::iterator::Signals;
    std::thread::spawn(move || {
        let mut signals = match Signals::new([SIGINT, SIGTERM]) {
            Ok(signals) => signals,
            Err(e) => {
                report!("failed to install signal handler: {}", e);
                return;
            },
        };
        if signals.forever().next().is_some() {
            poster.post(crate::app::AppEvent::Terminate);
        }
    });
}

pub fn detach_from_terminal(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

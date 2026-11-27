use crate::session::connection::Detacher;

pub fn detach_on_signal(detacher: Detacher) {
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
            if let Err(e) = detacher.detach() {
                report!(
                    "the signal could not be passed on to a session, \
                     so the window quits: {}",
                    e
                );
                std::process::exit(1);
            }
        }
    });
}

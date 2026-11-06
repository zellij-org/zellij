use crate::session::connection::Detacher;
use crate::windows_shell::console_events::end_on_console_event;

pub fn detach_on_signal(detacher: Detacher) {
    end_on_console_event(move || {
        if let Err(e) = detacher.detach() {
            report!(
                "the console event could not be passed on to a session: {}",
                e
            );
        }
    });
}

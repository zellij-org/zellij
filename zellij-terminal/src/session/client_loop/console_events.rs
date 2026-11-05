use std::sync::OnceLock;
use std::time::Duration;

use windows_sys::Win32::Foundation::BOOL;
use windows_sys::Win32::System::Console::{
    SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT,
};

use crate::session::connection::Detacher;

const CLOSE_GRACE: Duration = Duration::from_secs(3);

static DETACHER: OnceLock<Detacher> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Response {
    Detach,
    DetachAndWait,
    Ignore,
}

fn response_to(event: u32) -> Response {
    match event {
        CTRL_C_EVENT | CTRL_BREAK_EVENT => Response::Detach,
        CTRL_CLOSE_EVENT => Response::DetachAndWait,
        _ => Response::Ignore,
    }
}

pub fn detach_on_signal(detacher: Detacher) {
    if DETACHER.set(detacher).is_err() {
        return;
    }
    if unsafe { SetConsoleCtrlHandler(Some(on_event), 1) } == 0 {
        report!(
            "failed to watch the console, so Ctrl+C or closing it will end \
             the window abruptly"
        );
    }
}

unsafe extern "system" fn on_event(event: u32) -> BOOL {
    let response = response_to(event);
    if response == Response::Ignore {
        return 0;
    }
    if let Some(detacher) = DETACHER.get() {
        if let Err(e) = detacher.detach() {
            report!(
                "the console event could not be passed on to a session: {}",
                e
            );
        }
    }
    if response == Response::DetachAndWait {
        std::thread::sleep(CLOSE_GRACE);
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Console::{CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT};

    #[test]
    fn each_console_event_gets_the_right_response() {
        assert_eq!(
            response_to(CTRL_C_EVENT),
            Response::Detach,
            "Ctrl+C detaches and lets the window close the normal way"
        );
        assert_eq!(response_to(CTRL_BREAK_EVENT), Response::Detach);
        assert_eq!(
            response_to(CTRL_CLOSE_EVENT),
            Response::DetachAndWait,
            "Windows ends the process as soon as a close is handled, so the detach needs time"
        );
        assert_eq!(
            response_to(CTRL_LOGOFF_EVENT),
            Response::Ignore,
            "a process with windows hears about a session end through its windows"
        );
        assert_eq!(response_to(CTRL_SHUTDOWN_EVENT), Response::Ignore);
    }
}

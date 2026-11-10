use std::sync::OnceLock;
use std::time::Duration;

use windows_sys::Win32::Foundation::BOOL;
use windows_sys::Win32::System::Console::{
    SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT,
};

const CLOSE_GRACE: Duration = Duration::from_secs(3);

type Ending = Box<dyn Fn() + Send + Sync>;

static ENDING: OnceLock<Ending> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Response {
    End,
    EndAndWait,
    Ignore,
}

fn response_to(event: u32) -> Response {
    match event {
        CTRL_C_EVENT | CTRL_BREAK_EVENT => Response::End,
        CTRL_CLOSE_EVENT => Response::EndAndWait,
        _ => Response::Ignore,
    }
}

/// Runs `end` on Ctrl+C, Ctrl+Break or a closing console, where Windows would otherwise end
/// the process on the spot. Only the first call in a process takes effect.
pub fn end_on_console_event(end: impl Fn() + Send + Sync + 'static) {
    if ENDING.set(Box::new(end)).is_err() {
        return;
    }
    if unsafe { SetConsoleCtrlHandler(Some(on_event), 1) } == 0 {
        report!(
            "failed to watch the console, so Ctrl+C or closing it will end Zellij \
             abruptly"
        );
    }
}

unsafe extern "system" fn on_event(event: u32) -> BOOL {
    let response = response_to(event);
    if response == Response::Ignore {
        return 0;
    }
    if let Some(end) = ENDING.get() {
        end();
    }
    if response == Response::EndAndWait {
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
            Response::End,
            "Ctrl+C ends the windows and lets them close the normal way"
        );
        assert_eq!(response_to(CTRL_BREAK_EVENT), Response::End);
        assert_eq!(
            response_to(CTRL_CLOSE_EVENT),
            Response::EndAndWait,
            "Windows ends the process as soon as a close is handled, so ending needs time"
        );
        assert_eq!(
            response_to(CTRL_LOGOFF_EVENT),
            Response::Ignore,
            "a process with windows hears about a session end through its windows"
        );
        assert_eq!(response_to(CTRL_SHUTDOWN_EVENT), Response::Ignore);
    }
}

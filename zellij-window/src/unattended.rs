use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::windows::io::IntoRawHandle;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use windows_sys::Win32::System::Console::{
    GetConsoleProcessList, GetConsoleWindow, SetStdHandle, STD_ERROR_HANDLE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, MB_ICONERROR, MB_OK, MB_SETFOREGROUND,
};
use zellij_utils::consts::ZELLIJ_TMP_LOG_DIR;

use crate::identity::identity;

const LOG_LIMIT: u64 = 1024 * 1024;

pub struct Unattended {
    log: Option<PathBuf>,
}

pub fn prepare() -> Option<Unattended> {
    if !messages_vanish(console_has_window(), processes_on_console()) {
        return None;
    }
    let path = ZELLIJ_TMP_LOG_DIR.join(format!("{}-window.log", identity().app_id));
    let log = open_log(&path, LOG_LIMIT).ok().map(|file| {
        send_stderr_to(file);
        path
    });
    install_panic_hook(log.clone());
    Some(Unattended { log })
}

impl Unattended {
    pub fn report(&self, error: &anyhow::Error) {
        if crate::notice::was_shown() {
            return;
        }
        show(&failure_message(error, self.log.as_deref()));
    }
}

fn messages_vanish(console_has_window: bool, processes_on_console: u32) -> bool {
    !console_has_window || processes_on_console <= 1
}

fn console_has_window() -> bool {
    !unsafe { GetConsoleWindow() }.is_null()
}

fn processes_on_console() -> u32 {
    let mut ids = [0u32; 4];
    unsafe { GetConsoleProcessList(ids.as_mut_ptr(), ids.len() as u32) }
}

fn open_log(path: &Path, limit: u64) -> io::Result<File> {
    if fs::metadata(path).is_ok_and(|metadata| metadata.len() > limit) {
        let mut old = path.as_os_str().to_owned();
        old.push(".old");
        let _ = fs::rename(path, old);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}

fn send_stderr_to(mut file: File) {
    let _ = writeln!(
        file,
        "--- {} pid {}: {}",
        humantime::format_rfc3339_seconds(SystemTime::now()),
        std::process::id(),
        std::env::args().collect::<Vec<_>>().join(" ")
    );
    let handle = file.into_raw_handle();
    unsafe {
        SetStdHandle(STD_ERROR_HANDLE, handle);
    }
}

fn install_panic_hook(log: Option<PathBuf>) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default(info);
        show(&crash_message(&info.to_string(), log.as_deref()));
    }));
}

fn failure_message(error: &anyhow::Error, log: Option<&Path>) -> String {
    format!("{:#}{}", error, details(log))
}

fn crash_message(panic: &str, log: Option<&Path>) -> String {
    format!(
        "{} crashed.\n\n{}{}",
        identity().display_name,
        panic,
        details(log)
    )
}

fn details(log: Option<&Path>) -> String {
    match log {
        Some(log) => format!("\n\nMore details are in {}", log.display()),
        None => String::new(),
    }
}

fn show(message: &str) {
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() };
    let (text, caption) = (wide(message), wide(identity().display_name));
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{anyhow, Context};

    #[test]
    fn messages_vanish_without_a_console_window_or_on_a_console_of_our_own() {
        assert!(
            messages_vanish(false, 1),
            "the launcher's hidden console has no window"
        );
        assert!(
            messages_vanish(true, 1),
            "a console created for zellij alone closes when zellij exits"
        );
        assert!(
            messages_vanish(false, 0),
            "no console at all reports no processes"
        );
        assert!(
            !messages_vanish(true, 2),
            "a terminal's console is shared with its shell and stays open"
        );
    }

    #[test]
    fn the_log_is_appended_to_and_set_aside_once_it_is_too_long() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nested").join("zellij-window.log");

        writeln!(open_log(&path, 16).unwrap(), "first").unwrap();
        writeln!(open_log(&path, 16).unwrap(), "second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "first\nsecond\n");

        writeln!(open_log(&path, 8).unwrap(), "third").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "third\n");
        assert_eq!(
            fs::read_to_string(dir.path().join("nested").join("zellij-window.log.old")).unwrap(),
            "first\nsecond\n",
            "a log over the limit is kept as .old before a new one starts"
        );
    }

    #[test]
    fn a_failure_shows_its_whole_chain_and_where_the_details_are() {
        let error = Err::<(), _>(anyhow!("config.kdl:3: unexpected token"))
            .context("failed to read the configuration")
            .unwrap_err();
        let log = Path::new("logs").join("zellij-window.log");
        let message = failure_message(&error, Some(&log));
        assert!(
            message.starts_with("failed to read the configuration: config.kdl:3: unexpected token")
        );
        assert!(message.ends_with(&format!("More details are in {}", log.display())));
        assert_eq!(
            failure_message(&error, None),
            "failed to read the configuration: config.kdl:3: unexpected token",
            "without a log there is nowhere to point to"
        );
    }

    #[test]
    fn a_crash_names_the_application_and_the_panic() {
        let message = crash_message("panicked at src/window.rs:1:1:\nboom", None);
        assert_eq!(
            message,
            format!(
                "{} crashed.\n\npanicked at src/window.rs:1:1:\nboom",
                identity().display_name
            )
        );
    }
}

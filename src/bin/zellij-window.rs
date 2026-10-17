#![cfg_attr(windows, windows_subsystem = "windows")]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const LAUNCHER_SUFFIX: &str = "-window";

fn main() {
    if let Err(message) = run() {
        report(&message);
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let launcher = std::env::current_exe()
        .map_err(|e| format!("This launcher could not find its own location: {}", e))?;
    let program = target_of(&launcher).ok_or_else(|| {
        format!(
            "{} is not named <program>{}, so there is no program to start.",
            launcher.display(),
            LAUNCHER_SUFFIX
        )
    })?;
    if !program.is_file() {
        return Err(format!(
            "{} was not found. It has to sit next to {}.",
            program.display(),
            launcher.display()
        ));
    }
    let start_in = match (std::env::current_dir(), launcher.parent()) {
        (Ok(current), Some(launcher_dir)) => working_dir(
            &current,
            launcher_dir,
            system_dir().as_deref(),
            std::env::home_dir().as_deref(),
        ),
        _ => None,
    };
    launch(
        &program,
        &arguments(std::env::args_os().skip(1)),
        start_in.as_deref(),
    )
}

fn target_of(launcher: &Path) -> Option<PathBuf> {
    let name = launcher.file_name()?.to_str()?;
    let without_extension = strip_suffix_ignoring_case(name, std::env::consts::EXE_SUFFIX)?;
    let program = strip_suffix_ignoring_case(without_extension, LAUNCHER_SUFFIX)?;
    if program.is_empty() {
        return None;
    }
    Some(launcher.with_file_name(format!("{}{}", program, std::env::consts::EXE_SUFFIX)))
}

fn strip_suffix_ignoring_case<'a>(text: &'a str, suffix: &str) -> Option<&'a str> {
    let split = text.len().checked_sub(suffix.len())?;
    let matches = text.is_char_boundary(split) && text[split..].eq_ignore_ascii_case(suffix);
    matches.then(|| &text[..split])
}

fn arguments(forwarded: impl Iterator<Item = OsString>) -> Vec<OsString> {
    std::iter::once(OsString::from("window"))
        .chain(forwarded)
        .collect()
}

fn working_dir(
    current: &Path,
    launcher_dir: &Path,
    system_dir: Option<&Path>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    let started_where_nobody_works = same_dir(current, launcher_dir)
        || system_dir.is_some_and(|system_dir| same_dir(current, system_dir));
    if started_where_nobody_works {
        home.map(Path::to_path_buf)
    } else {
        None
    }
}

fn same_dir(a: &Path, b: &Path) -> bool {
    comparable(a) == comparable(b)
}

fn comparable(path: &Path) -> String {
    let text = path.to_string_lossy();
    let trimmed = text.trim_end_matches(['/', '\\']);
    if cfg!(windows) {
        trimmed.to_lowercase()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(windows)]
fn system_dir() -> Option<PathBuf> {
    std::env::var_os("SystemRoot").map(|root| PathBuf::from(root).join("System32"))
}

#[cfg(not(windows))]
fn system_dir() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn AllowSetForegroundWindow(process_id: u32) -> i32;
    fn MessageBoxW(
        window: *mut std::ffi::c_void,
        text: *const u16,
        caption: *const u16,
        kind: u32,
    ) -> i32;
}

#[cfg(windows)]
fn launch(program: &Path, arguments: &[OsString], start_in: Option<&Path>) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let mut command = Command::new(program);
    command
        .args(arguments)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(start_in) = start_in {
        command.current_dir(start_in);
    }
    let child = command
        .spawn()
        .map_err(|e| format!("{} could not be started: {}", program.display(), e))?;
    unsafe {
        AllowSetForegroundWindow(child.id());
    }
    Ok(())
}

#[cfg(unix)]
fn launch(program: &Path, arguments: &[OsString], start_in: Option<&Path>) -> Result<(), String> {
    use std::os::unix::process::CommandExt;

    let mut command = Command::new(program);
    command.args(arguments);
    if let Some(start_in) = start_in {
        command.current_dir(start_in);
    }
    let error = command.exec();
    Err(format!(
        "{} could not be started: {}",
        program.display(),
        error
    ))
}

#[cfg(windows)]
fn report(message: &str) {
    const MB_ICONERROR: u32 = 0x10;
    let caption = std::env::current_exe()
        .ok()
        .and_then(|launcher| target_of(&launcher))
        .and_then(|program| Some(program.file_stem()?.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "zellij".to_owned());
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() };
    let (text, caption) = (wide(message), wide(&caption));
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_ICONERROR,
        );
    }
}

#[cfg(not(windows))]
fn report(message: &str) {
    eprintln!("{}", message);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn executable(name: &str) -> String {
        format!("{}{}", name, std::env::consts::EXE_SUFFIX)
    }

    #[test]
    fn the_launcher_starts_the_program_it_is_named_after() {
        let dir = Path::new("install").join("bin");
        assert_eq!(
            target_of(&dir.join(executable("zellij-window"))),
            Some(dir.join(executable("zellij")))
        );
        assert_eq!(
            target_of(&dir.join(executable("acme-window"))),
            Some(dir.join(executable("acme"))),
            "a renamed launcher for another distribution starts that distribution"
        );
    }

    #[test]
    fn a_dot_or_capitals_in_the_name_do_not_confuse_the_target() {
        let dir = Path::new("install");
        assert_eq!(
            target_of(&dir.join(executable("zellij.dev-window"))),
            Some(dir.join(executable("zellij.dev")))
        );
        assert_eq!(
            target_of(&dir.join(format!(
                "ZELLIJ-WINDOW{}",
                std::env::consts::EXE_SUFFIX.to_uppercase()
            ))),
            Some(dir.join(executable("ZELLIJ")))
        );
    }

    #[test]
    fn a_launcher_not_named_after_a_program_starts_nothing() {
        let dir = Path::new("install");
        assert_eq!(target_of(&dir.join(executable("zellij"))), None);
        assert_eq!(target_of(&dir.join(executable("-window"))), None);
    }

    #[test]
    fn every_argument_is_passed_on_to_the_window_verb() {
        let forwarded = ["my-session", "--create"].map(OsString::from);
        assert_eq!(
            arguments(forwarded.into_iter()),
            ["window", "my-session", "--create"].map(OsString::from)
        );
        assert_eq!(arguments(std::iter::empty()), [OsString::from("window")]);
    }

    #[test]
    fn the_session_starts_at_home_rather_than_where_the_launcher_lives() {
        let launcher_dir = Path::new("install").join("bin");
        let system_dir = Path::new("os").join("system");
        let home = Path::new("users").join("me");
        let project = Path::new("users").join("me").join("project");

        assert_eq!(
            working_dir(&launcher_dir, &launcher_dir, Some(&system_dir), Some(&home)),
            Some(home.clone()),
            "a double-click starts the launcher in its own folder"
        );
        assert_eq!(
            working_dir(&system_dir, &launcher_dir, Some(&system_dir), Some(&home)),
            Some(home.clone())
        );
        assert_eq!(
            working_dir(&project, &launcher_dir, Some(&system_dir), Some(&home)),
            None,
            "a launch from a terminal keeps the terminal's folder"
        );
        assert_eq!(
            working_dir(&launcher_dir, &launcher_dir, None, None),
            None,
            "without a home folder the current one is kept"
        );
    }

    #[test]
    fn a_trailing_separator_does_not_make_a_different_folder() {
        let dir = Path::new("install").join("bin");
        let with_separator =
            PathBuf::from(format!("{}{}", dir.display(), std::path::MAIN_SEPARATOR));
        assert!(same_dir(&dir, &with_separator));
    }

    #[cfg(windows)]
    #[test]
    fn windows_folders_compare_without_regard_to_case() {
        assert!(same_dir(
            Path::new(r"C:\WINDOWS\system32"),
            Path::new(r"C:\Windows\System32")
        ));
    }
}

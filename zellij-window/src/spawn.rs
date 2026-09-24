use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use zellij_client::ServerLaunch;
use zellij_utils::consts::ZELLIJ_SOCK_DIR;
use zellij_utils::envs::{SESSION_NAME_ENV_KEY, ZELLIJ_ENV_KEY};
use zellij_utils::shared::set_permissions;

pub const ADVERTISED_TERM: &str = "xterm-256color";
pub const ADVERTISED_COLORTERM: &str = "truecolor";
pub const ADVERTISED_KITTY_WINDOW_ID: &str = "1";

const SOCK_DIR_PERMISSIONS: u32 = 0o700;

pub const LAUNCHING_SESSION_VARS: [&str; 3] =
    [ZELLIJ_ENV_KEY, SESSION_NAME_ENV_KEY, "ZELLIJ_PANE_ID"];

pub fn forget_launching_session() {
    for key in LAUNCHING_SESSION_VARS {
        std::env::remove_var(key);
    }
}

pub fn advertised_host_terminal_env() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("TERM".to_owned(), ADVERTISED_TERM.to_owned()),
        (
            "KITTY_WINDOW_ID".to_owned(),
            ADVERTISED_KITTY_WINDOW_ID.to_owned(),
        ),
    ])
}

pub fn resolve_program() -> Result<OsString> {
    std::env::current_exe()
        .map(PathBuf::into_os_string)
        .context("failed to locate this executable, which is also the session server")
}

pub fn server_command(
    program: OsString,
    socket_path: &Path,
    session_name: &str,
    cwd: &Path,
) -> ServerLaunch {
    let mut launch = ServerLaunch::new(program, socket_path);
    launch.env = BTreeMap::from([
        ("TERM".to_owned(), ADVERTISED_TERM.to_owned()),
        ("COLORTERM".to_owned(), ADVERTISED_COLORTERM.to_owned()),
        (SESSION_NAME_ENV_KEY.to_owned(), session_name.to_owned()),
    ]);
    launch.cwd = Some(cwd.to_owned());
    launch
}

pub fn resolve_cwd(requested: Option<PathBuf>) -> Result<PathBuf> {
    let cwd = match requested {
        Some(cwd) => cwd,
        None => std::env::current_dir().unwrap_or_else(|_| fallback_cwd()),
    };
    if !cwd.is_dir() {
        bail!("working directory {:?} is not a directory", cwd);
    }
    canonical(&cwd).with_context(|| format!("failed to resolve working directory {:?}", cwd))
}

#[cfg(windows)]
fn fallback_cwd() -> PathBuf {
    std::env::home_dir().unwrap_or_else(|| PathBuf::from("C:\\"))
}

#[cfg(not(windows))]
fn fallback_cwd() -> PathBuf {
    PathBuf::from("/")
}

#[cfg(windows)]
fn canonical(path: &Path) -> std::io::Result<PathBuf> {
    dunce::canonicalize(path)
}

#[cfg(not(windows))]
fn canonical(path: &Path) -> std::io::Result<PathBuf> {
    path.canonicalize()
}

pub fn prepare_socket_dir() -> Result<()> {
    let dir: &Path = &ZELLIJ_SOCK_DIR;
    fs::create_dir_all(dir).with_context(|| format!("failed to create {:?}", dir))?;
    set_permissions(dir, SOCK_DIR_PERMISSIONS)
        .with_context(|| format!("failed to restrict {:?} to the current user", dir))
}

pub fn spawn(launch: &ServerLaunch) -> Result<()> {
    zellij_client::spawn_server_with(launch).map_err(|e| {
        anyhow!(
            "{:?} could not be run or exited early, so the session server did not start: {}",
            launch.program,
            e
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command() -> ServerLaunch {
        server_command(
            OsString::from("/usr/bin/zellij"),
            Path::new("/run/zellij/sock/window-test"),
            "window-test",
            Path::new("/tmp/project"),
        )
    }

    #[test]
    fn the_server_is_started_the_way_the_client_starts_it() {
        let command = command();
        assert_eq!(command.program, OsString::from("/usr/bin/zellij"));
        assert_eq!(
            command.args(),
            vec![
                OsString::from("--server"),
                OsString::from("/run/zellij/sock/window-test")
            ]
        );
        assert_eq!(command.cwd, Some(PathBuf::from("/tmp/project")));
    }

    #[test]
    fn the_window_supplies_the_terminal_environment_a_host_would_have() {
        let env = command().env;
        assert_eq!(env.get("TERM").map(String::as_str), Some("xterm-256color"));
        assert_eq!(env.get("COLORTERM").map(String::as_str), Some("truecolor"));
    }

    #[test]
    fn the_session_name_reaches_panes_through_the_server_environment() {
        assert_eq!(
            command().env.get("ZELLIJ_SESSION_NAME").map(String::as_str),
            Some("window-test")
        );
    }

    #[test]
    fn the_session_a_window_was_launched_from_is_forgotten() {
        for key in LAUNCHING_SESSION_VARS {
            std::env::set_var(key, "launching-shell");
        }
        forget_launching_session();
        for key in LAUNCHING_SESSION_VARS {
            assert!(std::env::var_os(key).is_none(), "{} survived", key);
        }
    }

    #[test]
    fn the_shell_is_not_named_and_so_is_inherited() {
        assert!(!command().env.contains_key("SHELL"));
    }

    #[cfg(unix)]
    #[test]
    fn the_socket_directory_is_created_private_to_this_user_and_survives_a_second_call() {
        use std::os::unix::fs::PermissionsExt;

        prepare_socket_dir().unwrap();
        prepare_socket_dir().unwrap();

        let dir: &Path = &ZELLIJ_SOCK_DIR;
        let mode = fs::metadata(dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, SOCK_DIR_PERMISSIONS, "{:?} is {:o}", dir, mode);
    }

    #[test]
    fn the_session_server_is_this_very_executable() {
        assert_eq!(
            PathBuf::from(resolve_program().unwrap()),
            std::env::current_exe().unwrap()
        );
    }

    #[test]
    fn the_advertised_host_environment_names_the_terminal_the_window_emulates() {
        assert_eq!(
            advertised_host_terminal_env(),
            BTreeMap::from([
                ("TERM".to_owned(), "xterm-256color".to_owned()),
                ("KITTY_WINDOW_ID".to_owned(), "1".to_owned()),
            ])
        );
    }

    #[test]
    fn the_advertised_environment_asks_the_session_for_the_richer_notification_protocol() {
        use zellij_server::notifications::NotificationProtocol;
        use zellij_utils::input::options::HostNotificationProtocol;

        assert_eq!(
            NotificationProtocol::resolve(
                HostNotificationProtocol::Auto,
                &advertised_host_terminal_env()
            ),
            NotificationProtocol::Osc99,
            "the window reads OSC 99 metadata, so a notification reaches it with its title"
        );
        assert_eq!(
            NotificationProtocol::resolve(
                HostNotificationProtocol::Osc9,
                &advertised_host_terminal_env()
            ),
            NotificationProtocol::Osc9,
            "a configured protocol still wins"
        );
    }

    #[test]
    fn a_missing_working_directory_is_rejected_before_the_server_starts() {
        let dir = tempfile::TempDir::new().unwrap();
        let missing = dir.path().join("absent");
        assert!(resolve_cwd(Some(missing)).is_err());
    }

    #[test]
    fn a_requested_working_directory_is_resolved_to_an_absolute_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let nested = dir.path().join("nested");
        fs::create_dir(&nested).unwrap();
        let resolved = resolve_cwd(Some(nested.join(".").join("..").join("nested"))).unwrap();
        assert_eq!(resolved, canonical(&nested).unwrap());
    }

    #[test]
    fn no_requested_working_directory_means_the_one_the_window_was_launched_in() {
        assert_eq!(
            resolve_cwd(None).unwrap(),
            canonical(&std::env::current_dir().unwrap()).unwrap()
        );
    }

    #[test]
    fn a_program_that_cannot_be_run_is_named_in_the_error() {
        let err = spawn(&server_command(
            OsString::from("/nonexistent/zellij"),
            Path::new("/sock"),
            "name",
            Path::new("/"),
        ))
        .unwrap_err()
        .to_string();
        assert!(err.contains("/nonexistent/zellij"), "{}", err);
    }

    #[cfg(unix)]
    #[test]
    fn a_server_that_fails_to_start_is_reported_as_such() {
        let err = spawn(&ServerLaunch::new(
            OsString::from("false"),
            Path::new("/sock"),
        ))
        .unwrap_err()
        .to_string();
        assert!(err.contains("did not start"), "{}", err);
    }
}

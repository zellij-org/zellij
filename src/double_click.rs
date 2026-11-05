use std::path::{Path, PathBuf};

use zellij_utils::cli::{CliArgs, Command};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Platform {
    Windows,
    MacOs,
    Other,
}

impl Platform {
    fn current() -> Self {
        if cfg!(windows) {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else {
            Platform::Other
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Launch {
    pub platform: Platform,
    pub has_arguments: bool,
    pub has_terminal: bool,
    pub has_desktop: bool,
    pub over_ssh: bool,
    pub built_with_window: bool,
}

pub(crate) fn opens_window(launch: Launch) -> bool {
    let desktop_ok = match launch.platform {
        Platform::Windows => return false,
        Platform::MacOs => true,
        Platform::Other => launch.has_desktop,
    };
    desktop_ok
        && launch.built_with_window
        && !launch.has_arguments
        && !launch.has_terminal
        && !launch.over_ssh
}

pub(crate) fn starting_folder(
    current: &Path,
    program_dir: Option<&Path>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    let started_where_nobody_works =
        same_dir(current, Path::new("/")) || program_dir.is_some_and(|dir| same_dir(current, dir));
    if started_where_nobody_works {
        home.map(Path::to_path_buf)
    } else {
        None
    }
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let trim = |path: &Path| {
        let text = path.to_string_lossy().into_owned();
        let trimmed = text.trim_end_matches('/');
        if trimmed.is_empty() {
            "/".to_owned()
        } else {
            trimmed.to_owned()
        }
    };
    trim(a) == trim(b)
}

fn env_is_set(key: &str) -> bool {
    std::env::var_os(key).is_some_and(|value| !value.is_empty())
}

#[cfg(unix)]
fn has_terminal() -> bool {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .is_ok()
}

#[cfg(not(unix))]
fn has_terminal() -> bool {
    true
}

fn current_launch() -> Launch {
    Launch {
        platform: Platform::current(),
        has_arguments: std::env::args_os().len() > 1,
        has_terminal: has_terminal(),
        has_desktop: env_is_set("DISPLAY") || env_is_set("WAYLAND_DISPLAY"),
        over_ssh: env_is_set("SSH_CONNECTION"),
        built_with_window: cfg!(feature = "terminal"),
    }
}

fn window_opts() -> Option<CliArgs> {
    use clap::FromArgMatches;
    let command = CliArgs::command_for_distribution();
    let name = command.get_name().to_owned();
    let matches = command
        .try_get_matches_from([name.as_str(), "terminal"])
        .ok()?;
    let mut opts = CliArgs::from_arg_matches(&matches).ok()?;
    if let Some(Command::Terminal(terminal_args)) = opts.command.as_mut() {
        let current = std::env::current_dir().ok()?;
        let program = std::env::current_exe().ok();
        terminal_args.working_directory = starting_folder(
            &current,
            program.as_deref().and_then(Path::parent),
            std::env::home_dir().as_deref(),
        );
    }
    Some(opts)
}

pub(crate) fn reroute(opts: CliArgs) -> CliArgs {
    if !opens_window(current_launch()) {
        return opts;
    }
    window_opts().unwrap_or(opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn double_click(platform: Platform) -> Launch {
        Launch {
            platform,
            has_arguments: false,
            has_terminal: false,
            has_desktop: true,
            over_ssh: false,
            built_with_window: true,
        }
    }

    #[test]
    fn a_double_click_on_a_desktop_opens_the_window() {
        assert!(opens_window(double_click(Platform::Other)));
    }

    #[test]
    fn a_mac_app_opens_the_window_without_a_display_variable() {
        assert!(opens_window(Launch {
            has_desktop: false,
            ..double_click(Platform::MacOs)
        }));
    }

    #[test]
    fn windows_never_opens_the_window() {
        assert!(!opens_window(double_click(Platform::Windows)));
    }

    #[test]
    fn any_single_unmet_condition_keeps_the_old_behavior() {
        let base = double_click(Platform::Other);
        let cases = [
            Launch {
                has_arguments: true,
                ..base
            },
            Launch {
                has_terminal: true,
                ..base
            },
            Launch {
                has_desktop: false,
                ..base
            },
            Launch {
                over_ssh: true,
                ..base
            },
            Launch {
                built_with_window: false,
                ..base
            },
        ];
        for case in cases {
            assert!(!opens_window(case), "{:?}", case);
        }
        for case in [
            Launch {
                has_arguments: true,
                ..double_click(Platform::MacOs)
            },
            Launch {
                has_terminal: true,
                ..double_click(Platform::MacOs)
            },
            Launch {
                over_ssh: true,
                ..double_click(Platform::MacOs)
            },
        ] {
            assert!(!opens_window(case), "{:?}", case);
        }
    }

    #[test]
    fn the_session_starts_at_home_when_started_in_the_programs_folder_or_root() {
        let program_dir = Path::new("/opt/zellij/bin");
        let home = Path::new("/home/me");
        assert_eq!(
            starting_folder(program_dir, Some(program_dir), Some(home)),
            Some(home.to_path_buf())
        );
        assert_eq!(
            starting_folder(Path::new("/opt/zellij/bin/"), Some(program_dir), Some(home)),
            Some(home.to_path_buf())
        );
        assert_eq!(
            starting_folder(Path::new("/"), Some(program_dir), Some(home)),
            Some(home.to_path_buf())
        );
        assert_eq!(
            starting_folder(Path::new("/home/me/project"), Some(program_dir), Some(home)),
            None
        );
        assert_eq!(starting_folder(program_dir, Some(program_dir), None), None);
    }
}

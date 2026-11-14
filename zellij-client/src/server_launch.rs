use std::collections::BTreeMap;
use std::env::current_exe;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchStyle {
    WaitForDaemon,
    Detached,
}

pub const fn launch_style(windows: bool) -> LaunchStyle {
    if windows {
        LaunchStyle::Detached
    } else {
        LaunchStyle::WaitForDaemon
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerLaunch {
    pub program: OsString,
    pub socket_path: PathBuf,
    pub debug: bool,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<PathBuf>,
}

impl ServerLaunch {
    pub fn new(program: OsString, socket_path: &Path) -> Self {
        Self {
            program,
            socket_path: socket_path.to_owned(),
            debug: false,
            env: BTreeMap::new(),
            cwd: None,
        }
    }

    pub fn args(&self) -> Vec<OsString> {
        let mut args = vec![
            OsString::from("--server"),
            self.socket_path.as_os_str().to_owned(),
        ];
        if self.debug {
            args.push(OsString::from("--debug"));
        }
        args
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(self.args()).envs(&self.env);
        if let Some(cwd) = &self.cwd {
            cmd.current_dir(cwd);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
            cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
        }
        cmd
    }
}

pub fn spawn_server(socket_path: &Path, debug: bool) -> io::Result<()> {
    let mut launch = ServerLaunch::new(current_exe()?.into_os_string(), socket_path);
    launch.debug = debug;
    spawn_server_with(&launch)
}

pub fn spawn_server_with(launch: &ServerLaunch) -> io::Result<()> {
    let mut cmd = launch.command();
    match launch_style(cfg!(windows)) {
        LaunchStyle::WaitForDaemon => {
            let status = cmd.status()?;
            if status.success() {
                Ok(())
            } else {
                let msg = "Process returned non-zero exit code";
                let err_msg = match status.code() {
                    Some(c) => format!("{}: {}", msg, c),
                    None => msg.to_string(),
                };
                Err(io::Error::new(io::ErrorKind::Other, err_msg))
            }
        },
        LaunchStyle::Detached => {
            // The server outlives this process. Holding this process's stdio would keep a caller
            // that reads this process's output waiting until the server exits.
            #[cfg(windows)]
            let _withheld = WithheldStdio::new();
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            Ok(())
        },
    }
}

/// A child process on Windows inherits every inheritable handle of its parent, not only the
/// stdio it's given, so this process's own stdio is made non-inheritable while it lasts.
#[cfg(windows)]
struct WithheldStdio(Vec<windows_sys::Win32::Foundation::HANDLE>);

#[cfg(windows)]
impl WithheldStdio {
    fn new() -> Self {
        use windows_sys::Win32::Foundation::{
            GetHandleInformation, SetHandleInformation, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE,
        };
        use windows_sys::Win32::System::Console::{
            GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        };
        let mut withheld = Vec::new();
        for id in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            let handle = unsafe { GetStdHandle(id) };
            if handle.is_null() || handle == INVALID_HANDLE_VALUE || withheld.contains(&handle) {
                continue;
            }
            let mut flags = 0;
            let inheritable = unsafe { GetHandleInformation(handle, &mut flags) } != 0
                && flags & HANDLE_FLAG_INHERIT != 0;
            if inheritable && unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) } != 0 {
                withheld.push(handle);
            }
        }
        WithheldStdio(withheld)
    }
}

#[cfg(windows)]
impl Drop for WithheldStdio {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT};
        for handle in &self.0 {
            unsafe { SetHandleInformation(*handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch() -> ServerLaunch {
        ServerLaunch::new(
            OsString::from("/usr/bin/zellij"),
            Path::new("/run/zellij/sock/name"),
        )
    }

    #[test]
    fn unix_waits_for_the_daemonizing_server_and_windows_does_not() {
        assert_eq!(launch_style(false), LaunchStyle::WaitForDaemon);
        assert_eq!(launch_style(true), LaunchStyle::Detached);
    }

    #[test]
    fn a_plain_launch_is_exactly_the_attach_command_line() {
        let command = launch().command();
        assert_eq!(command.get_program(), "/usr/bin/zellij");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec!["--server", "/run/zellij/sock/name"]
        );
        assert_eq!(command.get_envs().count(), 0);
        assert_eq!(command.get_current_dir(), None);
    }

    #[test]
    fn debug_is_passed_after_the_socket_path() {
        let mut launch = launch();
        launch.debug = true;
        assert_eq!(
            launch.command().get_args().collect::<Vec<_>>(),
            vec!["--server", "/run/zellij/sock/name", "--debug"]
        );
    }

    #[test]
    fn extra_environment_and_working_directory_reach_the_server() {
        let mut launch = launch();
        launch
            .env
            .insert("TERM".to_owned(), "xterm-256color".to_owned());
        launch.cwd = Some(PathBuf::from("/tmp/project"));
        let command = launch.command();
        assert_eq!(
            command.get_envs().collect::<Vec<_>>(),
            vec![(
                std::ffi::OsStr::new("TERM"),
                Some(std::ffi::OsStr::new("xterm-256color"))
            )]
        );
        assert_eq!(command.get_current_dir(), Some(Path::new("/tmp/project")));
    }

    #[cfg(unix)]
    #[test]
    fn a_server_that_exits_unsuccessfully_is_an_error() {
        let launch = ServerLaunch::new(OsString::from("false"), Path::new("/sock"));
        assert!(spawn_server_with(&launch).is_err());
    }
}

//! Zellij program-wide constants.

use crate::home::find_default_config_dir;
use directories::ProjectDirs;
use include_dir::{include_dir, Dir};
use lazy_static::lazy_static;
use std::{path::PathBuf, sync::OnceLock};
use uuid::Uuid;

pub const ZELLIJ_CONFIG_FILE_ENV: &str = "ZELLIJ_CONFIG_FILE";
pub const ZELLIJ_CONFIG_DIR_ENV: &str = "ZELLIJ_CONFIG_DIR";
pub const ZELLIJ_LAYOUT_DIR_ENV: &str = "ZELLIJ_LAYOUT_DIR";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_SCROLL_BUFFER_SIZE: usize = 10_000;
pub static SCROLL_BUFFER_SIZE: OnceLock<usize> = OnceLock::new();
pub static DEBUG_MODE: OnceLock<bool> = OnceLock::new();

/// System-wide configuration directory, named after the running distribution
/// (`/etc/zellij`, `C:\ProgramData\Zellij`)
#[cfg(not(windows))]
pub fn system_default_config_dir() -> PathBuf {
    PathBuf::from("/etc").join(crate::distribution::name())
}

#[cfg(windows)]
pub fn system_default_config_dir() -> PathBuf {
    PathBuf::from("C:\\ProgramData").join(crate::distribution::display_name())
}

pub const SYSTEM_DEFAULT_DATA_DIR_PREFIX: &str = system_default_data_dir();

pub static ZELLIJ_DEFAULT_THEMES: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/themes");

pub const DEFAULT_THEME_NAME: &str = "default";
pub const DEFAULT_LIGHT_THEME_NAME: &str = "default-light";

pub const CLIENT_SERVER_CONTRACT_VERSION: usize = 1;

// The session-info cache is keyed by session id (a short random id for new sessions,
// or the session name for sessions from before ids), NOT the user-visible
// display name — so renaming a session never moves its cache folder.
pub fn session_info_cache_file_name(session_id: &str) -> PathBuf {
    session_info_folder_for_session(session_id).join("session-metadata.kdl")
}

pub fn session_layout_cache_file_name(session_id: &str) -> PathBuf {
    session_info_folder_for_session(session_id).join("session-layout.kdl")
}

pub fn session_info_folder_for_session(session_id: &str) -> PathBuf {
    ZELLIJ_SESSION_INFO_CACHE_DIR.join(session_id)
}

/// Tombstone sibling of a session-info folder. Lives *outside* the folder so
/// it survives `remove_dir_all` and fences late metadata/layout writers that
/// would otherwise republish a deleted session (#5709).
pub fn session_deletion_tombstone_path(session_id: &str) -> PathBuf {
    ZELLIJ_SESSION_INFO_CACHE_DIR.join(format!("{session_id}.deleted"))
}

/// Sibling of the session directory. Writers and deleters share this lock so
/// it outlives `remove_dir_all` of the session folder. One file per session id.
pub fn session_publication_lock_path(session_id: &str) -> PathBuf {
    ZELLIJ_SESSION_INFO_CACHE_DIR.join(format!("{session_id}.lock"))
}

/// Sibling incarnation file. A writer stores the value it read at creation and
/// may publish only while the file still contains that value.
pub fn session_incarnation_path(session_id: &str) -> PathBuf {
    ZELLIJ_SESSION_INFO_CACHE_DIR.join(format!("{session_id}.incarnation"))
}

/// Length of a generated session id (hex chars). Kept short so the socket/pipe
/// path stays within the platform limit (104 bytes on macOS) even under a long
/// temp dir — a full 36-char UUID would overflow it. The client checks the socket
/// directory against this length at startup. See `generate_session_id`.
pub const SESSION_ID_LENGTH: usize = 12;

pub fn create_config_and_cache_folders() {
    if let Err(e) = std::fs::create_dir_all(&ZELLIJ_CACHE_DIR.as_path()) {
        log::error!("Failed to create cache dir: {:?}", e);
    }
    if let Some(config_dir) = find_default_config_dir() {
        if let Err(e) = std::fs::create_dir_all(&config_dir.as_path()) {
            log::error!("Failed to create config dir: {:?}", e);
        }
    }
    // while session_info is a child of cache currently, it won't necessarily always be this way,
    // and so it's explicitly created here
    if let Err(e) = std::fs::create_dir_all(&ZELLIJ_SESSION_INFO_CACHE_DIR.as_path()) {
        log::error!("Failed to create session_info cache dir: {:?}", e);
    }
    prune_empty_session_info_folders();
}

fn prune_empty_session_info_folders() {
    let Ok(entries) = std::fs::read_dir(&*ZELLIJ_SESSION_INFO_CACHE_DIR) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let is_empty = std::fs::read_dir(&path)
            .ok()
            .map_or(false, |mut iter| iter.next().is_none());
        if is_empty {
            if let Err(e) = std::fs::remove_dir(&path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    log::debug!("Failed to prune empty session folder {:?}: {:?}", path, e);
                }
            }
        }
    }
}

const fn system_default_data_dir() -> &'static str {
    if let Some(data_dir) = std::option_env!("PREFIX") {
        data_dir
    } else if cfg!(windows) {
        "C:\\ProgramData\\Zellij"
    } else {
        "/usr"
    }
}

lazy_static! {
    pub static ref CLIENT_SERVER_CONTRACT_DIR: String =
        format!("contract_version_{}", CLIENT_SERVER_CONTRACT_VERSION);
    pub static ref ZELLIJ_PROJ_DIR: ProjectDirs = {
        let (qualifier, organization, application) =
            crate::distribution::distribution().project_dirs();
        ProjectDirs::from(qualifier, organization, application).unwrap_or_else(|| {
            panic!(
                "could not determine the platform directories for distribution '{}' \
                 (qualifier: {:?}, organization: {:?}, application: {:?})",
                crate::distribution::name(),
                qualifier,
                organization,
                application
            )
        })
    };
    pub static ref ZELLIJ_CACHE_DIR: PathBuf = ZELLIJ_PROJ_DIR.cache_dir().to_path_buf();
    pub static ref ZELLIJ_SESSION_CACHE_DIR: PathBuf = ZELLIJ_PROJ_DIR
        .cache_dir()
        .to_path_buf()
        .join(format!("{}", Uuid::new_v4()));
    pub static ref ZELLIJ_PLUGIN_PERMISSIONS_CACHE: PathBuf =
        ZELLIJ_CACHE_DIR.join("permissions.kdl");
    pub static ref ZELLIJ_SESSION_INFO_CACHE_DIR: PathBuf = ZELLIJ_CACHE_DIR
        .join(CLIENT_SERVER_CONTRACT_DIR.clone())
        .join("session_info");
    pub static ref ZELLIJ_PLUGIN_ARTIFACT_DIR: PathBuf = ZELLIJ_CACHE_DIR.join(VERSION);
    pub static ref ZELLIJ_SEEN_RELEASE_NOTES_CACHE_FILE: PathBuf =
        ZELLIJ_CACHE_DIR.join(VERSION).join("seen_release_notes");
}

pub const FEATURES: &[&str] = &[
    #[cfg(feature = "disable_automatic_asset_installation")]
    "disable_automatic_asset_installation",
];

#[cfg(unix)]
pub fn is_ipc_socket(file_type: &std::fs::FileType) -> bool {
    use std::os::unix::fs::FileTypeExt;
    file_type.is_socket()
}

#[cfg(not(unix))]
pub fn is_ipc_socket(file_type: &std::fs::FileType) -> bool {
    file_type.is_file()
}

#[cfg(unix)]
pub fn ipc_connect(path: &std::path::Path) -> std::io::Result<interprocess::local_socket::Stream> {
    use interprocess::local_socket::{prelude::*, GenericFilePath, Stream as LocalSocketStream};
    let fs_name = path.to_fs_name::<GenericFilePath>()?;
    LocalSocketStream::connect(fs_name)
}

#[cfg(windows)]
pub fn ipc_connect(path: &std::path::Path) -> std::io::Result<interprocess::local_socket::Stream> {
    use interprocess::local_socket::{prelude::*, GenericNamespaced, Stream as LocalSocketStream};
    let name = path.to_string_lossy().to_string();
    let ns_name = name.to_ns_name::<GenericNamespaced>()?;
    LocalSocketStream::connect(ns_name)
}

/// Outcome of one nonblocking Unix connect poll.
///
/// `so_error` is the raw `SO_ERROR` value (`0` means no pending error).
#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UnixConnectPoll {
    pollout: bool,
    pollhup: bool,
    pollerr: bool,
    pollnval: bool,
    so_error: i32,
}

#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnixConnectReadiness {
    Connected,
    Pending,
    Failed(i32),
}

/// Classify a nonblocking Unix connect after `poll` reports readiness.
///
/// `SO_ERROR == 0` with `POLLOUT` or `POLLHUP` is connected. macOS `poll` reports
/// `POLLHUP` without `POLLOUT` while `SO_ERROR` is still 0 for a Unix socket that
/// has connected and is waiting for `accept`. Treating that as a timeout drops a
/// later `Connected` reply and the probe becomes `Unknown`.
#[cfg(unix)]
fn classify_unix_connect_readiness(poll: UnixConnectPoll) -> UnixConnectReadiness {
    use nix::errno::Errno;

    if poll.pollnval {
        return UnixConnectReadiness::Failed(Errno::EBADF as i32);
    }
    if poll.so_error != 0 {
        return match Errno::from_raw(poll.so_error) {
            Errno::EINPROGRESS | Errno::EALREADY | Errno::EAGAIN | Errno::EINTR => {
                UnixConnectReadiness::Pending
            },
            _ => UnixConnectReadiness::Failed(poll.so_error),
        };
    }
    if poll.pollout || poll.pollhup {
        UnixConnectReadiness::Connected
    } else if poll.pollerr {
        UnixConnectReadiness::Failed(Errno::EIO as i32)
    } else {
        UnixConnectReadiness::Pending
    }
}

#[cfg(unix)]
fn set_cloexec_and_nonblock(fd: &std::os::fd::OwnedFd) -> std::io::Result<()> {
    use nix::fcntl::{fcntl, FcntlArg, FdFlag, OFlag};

    let status = OFlag::from_bits_truncate(fcntl(fd, FcntlArg::F_GETFL)?);
    fcntl(fd, FcntlArg::F_SETFL(status | OFlag::O_NONBLOCK))?;
    let descriptor = FdFlag::from_bits_truncate(fcntl(fd, FcntlArg::F_GETFD)?);
    fcntl(fd, FcntlArg::F_SETFD(descriptor | FdFlag::FD_CLOEXEC))?;
    Ok(())
}

#[cfg(unix)]
fn clear_nonblock(fd: &std::os::fd::OwnedFd) -> std::io::Result<()> {
    use nix::fcntl::{fcntl, FcntlArg, OFlag};

    let status = OFlag::from_bits_truncate(fcntl(fd, FcntlArg::F_GETFL)?);
    fcntl(fd, FcntlArg::F_SETFL(status.difference(OFlag::O_NONBLOCK)))?;
    Ok(())
}

#[cfg(unix)]
fn wait_for_unix_connect(
    fd: &std::os::fd::OwnedFd,
    deadline: std::time::Instant,
) -> std::io::Result<()> {
    use nix::errno::Errno;
    use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
    use nix::sys::socket::{getpeername, getsockopt, sockopt, UnixAddr};
    use std::os::fd::{AsFd, AsRawFd};
    use std::time::Instant;

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "ipc connect timed out",
            ));
        }
        let poll_timeout = PollTimeout::try_from(remaining).unwrap_or(PollTimeout::MAX);
        let mut fds = [PollFd::new(fd.as_fd(), PollFlags::POLLOUT)];
        match poll(&mut fds, poll_timeout) {
            Err(Errno::EINTR) => continue,
            Err(error) => return Err(error.into()),
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "ipc connect timed out",
                ));
            },
            Ok(_) => {
                let revents = fds[0].revents().unwrap_or(PollFlags::empty());
                let so_error = getsockopt(fd, sockopt::SocketError)?;
                match classify_unix_connect_readiness(UnixConnectPoll {
                    pollout: revents.contains(PollFlags::POLLOUT),
                    pollhup: revents.contains(PollFlags::POLLHUP),
                    pollerr: revents.contains(PollFlags::POLLERR),
                    pollnval: revents.contains(PollFlags::POLLNVAL),
                    so_error,
                }) {
                    UnixConnectReadiness::Connected => {
                        // A writable/hangup readiness with SO_ERROR == 0 is not
                        // by itself proof of a peer. Require the kernel to name
                        // the connected peer before handing the socket out.
                        getpeername::<UnixAddr>(fd.as_raw_fd()).map_err(std::io::Error::from)?;
                        return Ok(());
                    },
                    UnixConnectReadiness::Pending => continue,
                    UnixConnectReadiness::Failed(code) => {
                        return Err(std::io::Error::from_raw_os_error(code));
                    },
                }
            },
        }
    }
}

/// Connect under one absolute deadline. A full Unix accept queue can return
/// EAGAIN without starting a connection; polling that socket is not evidence
/// that it is connected. Only EINPROGRESS/EALREADY enter the poll-completion
/// path. Retry EAGAIN with a fresh descriptor and the original deadline.
#[cfg(unix)]
pub fn ipc_connect_with_timeout(
    path: &std::path::Path,
    timeout: std::time::Duration,
) -> std::io::Result<interprocess::local_socket::Stream> {
    use interprocess::local_socket::Stream as LocalSocketStream;
    use nix::errno::Errno;
    use nix::sys::socket::{connect, socket, AddressFamily, SockFlag, SockType, UnixAddr};
    use std::os::fd::{AsRawFd, OwnedFd};
    use std::time::{Duration, Instant};

    if timeout.is_zero() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "ipc connect timed out",
        ));
    }
    let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "ipc connect timeout is too large",
        )
    })?;
    let address = UnixAddr::new(path)?;
    let timed_out = || std::io::Error::new(std::io::ErrorKind::TimedOut, "ipc connect timed out");
    'attempt: loop {
        if Instant::now() >= deadline {
            return Err(timed_out());
        }
        let fd: OwnedFd = socket(
            AddressFamily::Unix,
            SockType::Stream,
            SockFlag::empty(),
            None,
        )?;
        set_cloexec_and_nonblock(&fd)?;
        loop {
            if Instant::now() >= deadline {
                return Err(timed_out());
            }
            match connect(fd.as_raw_fd(), &address) {
                Ok(()) | Err(Errno::EISCONN) => break,
                Err(Errno::EINTR) => continue,
                Err(Errno::EAGAIN) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Err(timed_out());
                    }
                    std::thread::sleep(Duration::from_millis(1).min(remaining));
                    continue 'attempt;
                },
                Err(Errno::EINPROGRESS) | Err(Errno::EALREADY) => {
                    match wait_for_unix_connect(&fd, deadline) {
                        Ok(()) => break,
                        Err(error) if error.raw_os_error() == Some(Errno::ENOTCONN as i32) => {
                            let remaining = deadline.saturating_duration_since(Instant::now());
                            if remaining.is_zero() {
                                return Err(timed_out());
                            }
                            std::thread::sleep(Duration::from_millis(1).min(remaining));
                            continue 'attempt;
                        },
                        Err(error) => return Err(error),
                    }
                },
                Err(error) => return Err(error.into()),
            }
        }
        clear_nonblock(&fd)?;
        return Ok(LocalSocketStream::UdSocket(fd.into()));
    }
}

/// Windows named-pipe connect does not honour `ConnectWaitMode::Timeout` the same way Unix does,
/// but probes still go through this API so the call site always passes the remaining deadline.
#[cfg(windows)]
pub fn ipc_connect_with_timeout(
    path: &std::path::Path,
    timeout: std::time::Duration,
) -> std::io::Result<interprocess::local_socket::Stream> {
    use interprocess::{
        local_socket::{prelude::*, ConnectOptions, GenericNamespaced},
        ConnectWaitMode,
    };
    let name = path.to_string_lossy().to_string();
    let ns_name = name.to_ns_name::<GenericNamespaced>()?;
    ConnectOptions::new()
        .name(ns_name)
        .wait_mode(ConnectWaitMode::Timeout(timeout))
        .connect_sync()
}

#[cfg(unix)]
pub fn ipc_bind(path: &std::path::Path) -> std::io::Result<interprocess::local_socket::Listener> {
    use interprocess::local_socket::{prelude::*, GenericFilePath, ListenerOptions};
    let fs_name = path.to_fs_name::<GenericFilePath>()?;
    ListenerOptions::new().name(fs_name).create_sync()
}

#[cfg(windows)]
pub fn ipc_bind(path: &std::path::Path) -> std::io::Result<interprocess::local_socket::Listener> {
    use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions};
    let name = path.to_string_lossy().to_string();
    let ns_name = name.to_ns_name::<GenericNamespaced>()?;
    let listener = ListenerOptions::new().name(ns_name).create_sync()?;
    std::fs::write(path, std::process::id().to_string())?;
    Ok(listener)
}

#[cfg(unix)]
pub fn ipc_bind_async(
    path: &std::path::Path,
) -> std::io::Result<interprocess::local_socket::tokio::Listener> {
    use interprocess::local_socket::{prelude::*, GenericFilePath, ListenerOptions};
    let fs_name = path.to_fs_name::<GenericFilePath>()?;
    ListenerOptions::new().name(fs_name).create_tokio()
}

#[cfg(windows)]
pub fn ipc_bind_async(
    path: &std::path::Path,
) -> std::io::Result<interprocess::local_socket::tokio::Listener> {
    use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions};
    let name = path.to_string_lossy().to_string();
    let ns_name = name.to_ns_name::<GenericNamespaced>()?;
    let listener = ListenerOptions::new().name(ns_name).create_tokio()?;
    std::fs::write(path, std::process::id().to_string())?;
    Ok(listener)
}

#[cfg(windows)]
pub fn ipc_connect_reply(
    path: &std::path::Path,
) -> std::io::Result<interprocess::local_socket::Stream> {
    use interprocess::local_socket::{prelude::*, GenericNamespaced, Stream as LocalSocketStream};
    let name = format!("{}-reply", path.to_string_lossy());
    let ns_name = name.to_ns_name::<GenericNamespaced>()?;
    LocalSocketStream::connect(ns_name)
}

#[cfg(windows)]
pub fn ipc_connect_reply_with_timeout(
    path: &std::path::Path,
    timeout: std::time::Duration,
) -> std::io::Result<interprocess::local_socket::Stream> {
    use interprocess::{
        local_socket::{prelude::*, ConnectOptions, GenericNamespaced},
        ConnectWaitMode,
    };
    if timeout.is_zero() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "ipc connect timed out",
        ));
    }
    let name = format!("{}-reply", path.to_string_lossy());
    let ns_name = name.to_ns_name::<GenericNamespaced>()?;
    ConnectOptions::new()
        .name(ns_name)
        .wait_mode(ConnectWaitMode::Timeout(timeout))
        .connect_sync()
}

#[cfg(windows)]
pub fn ipc_bind_reply(
    path: &std::path::Path,
) -> std::io::Result<interprocess::local_socket::Listener> {
    use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions};
    let name = format!("{}-reply", path.to_string_lossy());
    let ns_name = name.to_ns_name::<GenericNamespaced>()?;
    ListenerOptions::new().name(ns_name).create_sync()
}

// Kept short: macOS limits socket paths to 104 bytes, and the temp dir already uses about half.
pub const APP_DIR_NAME: &str = ".desk";
pub const APP_CONTROL_SOCKET_NAME: &str = "control";
pub const APP_ATTENTION_SOCKET_NAME: &str = "attention";
pub const APP_LOCK_FILE_NAME: &str = "lock";
pub const APP_PANEL_SESSION: &str = ".zj-panel";
pub const APP_QUIT_REQUEST: &str = ".zellij-app-quit";

pub fn is_app_socket_name(name: &str) -> bool {
    name.starts_with('.')
}

pub fn app_dir_in(sock_dir: &std::path::Path) -> PathBuf {
    sock_dir.join(APP_DIR_NAME)
}

pub fn app_dir() -> PathBuf {
    app_dir_in(&ZELLIJ_SOCK_DIR)
}

pub fn app_control_socket() -> PathBuf {
    app_dir().join(APP_CONTROL_SOCKET_NAME)
}

pub fn app_attention_socket() -> PathBuf {
    app_dir().join(APP_ATTENTION_SOCKET_NAME)
}

pub fn app_lock_file() -> PathBuf {
    app_dir().join(APP_LOCK_FILE_NAME)
}

pub fn session_socket_path_in(sock_dir: &std::path::Path, session_name: &str) -> PathBuf {
    if session_name == APP_PANEL_SESSION {
        app_dir_in(sock_dir).join(session_name)
    } else {
        sock_dir.join(session_name)
    }
}

#[cfg(not(target_family = "wasm"))]
pub fn session_socket_path(session_name: &str) -> Result<PathBuf, String> {
    match crate::sessions::resolve_session_socket_path(session_name)? {
        Some(path) => Ok(path),
        None => Ok(session_socket_path_in(&ZELLIJ_SOCK_DIR, session_name)),
    }
}

#[cfg(not(target_family = "wasm"))]
pub fn new_session_socket_path_in(sock_dir: &std::path::Path, session_name: &str) -> PathBuf {
    if is_app_socket_name(session_name) {
        session_socket_path_in(sock_dir, session_name)
    } else {
        sock_dir.join(crate::sessions::register_session(session_name))
    }
}

pub fn session_id_socket_path(session_id: &str) -> PathBuf {
    session_socket_path_in(&ZELLIJ_SOCK_DIR, session_id)
}

pub fn prepare_private_dir(dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    crate::shared::set_permissions(dir, 0o700)
}

#[cfg(unix)]
pub use unix_only::*;

#[cfg(unix)]
mod unix_only {
    use super::*;
    use crate::envs;
    pub use crate::shared::set_permissions;
    use lazy_static::lazy_static;
    use nix::unistd::Uid;
    use std::env::temp_dir;

    // Maximum sockaddr_un.sun_path length: 104 on macOS/BSD, 108 on Linux/Android/Solaris.
    #[cfg(target_os = "macos")]
    pub const ZELLIJ_SOCK_MAX_LENGTH: usize = 104;
    #[cfg(not(target_os = "macos"))]
    pub const ZELLIJ_SOCK_MAX_LENGTH: usize = 108;

    lazy_static! {
        static ref UID: Uid = Uid::current();
        pub static ref ZELLIJ_TMP_DIR: PathBuf =
            temp_dir().join(format!("{}-{}", crate::distribution::name(), *UID));
        pub static ref ZELLIJ_TMP_LOG_DIR: PathBuf =
            ZELLIJ_TMP_DIR.join(format!("{}-log", crate::distribution::name()));
        pub static ref ZELLIJ_TMP_LOG_FILE: PathBuf =
            ZELLIJ_TMP_LOG_DIR.join(format!("{}.log", crate::distribution::name()));
        pub static ref ZELLIJ_SOCK_DIR: PathBuf = {
            let mut ipc_dir = envs::get_socket_dir().map_or_else(
                |_| {
                    ZELLIJ_PROJ_DIR
                        .runtime_dir()
                        .map_or_else(|| ZELLIJ_TMP_DIR.clone(), |p| p.to_owned())
                },
                PathBuf::from,
            );
            ipc_dir.push(CLIENT_SERVER_CONTRACT_DIR.clone());
            ipc_dir
        };
        pub static ref WEBSERVER_SOCKET_PATH: PathBuf = ZELLIJ_SOCK_DIR.join("web_server_bus");
    }
}

#[cfg(not(unix))]
pub use not_unix::*;

#[cfg(not(unix))]
mod not_unix {
    use super::*;
    use crate::envs;
    pub use crate::shared::set_permissions;
    #[cfg(windows)]
    use dunce;
    use lazy_static::lazy_static;
    use std::env::temp_dir;

    #[cfg(windows)]
    fn canonicalize_path(path: PathBuf) -> PathBuf {
        dunce::canonicalize(&path).unwrap_or(path)
    }

    #[cfg(not(windows))]
    fn canonicalize_path(path: PathBuf) -> PathBuf {
        path
    }

    pub const ZELLIJ_SOCK_MAX_LENGTH: usize = 256;

    lazy_static! {
        pub static ref ZELLIJ_TMP_DIR: PathBuf = {
            let tmp_dir = canonicalize_path(temp_dir());
            tmp_dir.join(crate::distribution::name())
        };
        pub static ref ZELLIJ_TMP_LOG_DIR: PathBuf =
            ZELLIJ_TMP_DIR.join(format!("{}-log", crate::distribution::name()));
        pub static ref ZELLIJ_TMP_LOG_FILE: PathBuf =
            ZELLIJ_TMP_LOG_DIR.join(format!("{}.log", crate::distribution::name()));
        pub static ref ZELLIJ_SOCK_DIR: PathBuf = {
            let mut ipc_dir = canonicalize_path(envs::get_socket_dir().map_or_else(
                |_| {
                    ZELLIJ_PROJ_DIR
                        .runtime_dir()
                        .map_or_else(|| ZELLIJ_TMP_DIR.clone(), |p| p.to_owned())
                },
                PathBuf::from,
            ));
            ipc_dir.push(CLIENT_SERVER_CONTRACT_DIR.clone());
            ipc_dir
        };
        pub static ref WEBSERVER_SOCKET_PATH: PathBuf = ZELLIJ_SOCK_DIR.join("web_server_bus");
    }
}

#[cfg(test)]
mod app_dir_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn every_desktop_app_socket_fits_the_macos_limit_with_room_to_spare() {
        const MACOS_SOCKET_PATH_LIMIT: usize = 104;
        // Leaves room for a longer uid and contract version than we have today.
        let sock_dir = Path::new(
            "/var/folders/g_/w_mkp3kj0jnbzm398bln58rm0000gn/T/zellij-50100/contract_version_100",
        );
        let app = app_dir_in(sock_dir);
        for path in [
            session_socket_path_in(sock_dir, APP_PANEL_SESSION),
            app.join(APP_CONTROL_SOCKET_NAME),
            app.join(APP_ATTENTION_SOCKET_NAME),
            app.join("windows").join("99999"),
        ] {
            let length = path.as_os_str().len();
            assert!(
                length < MACOS_SOCKET_PATH_LIMIT,
                "{:?} is {} bytes",
                path,
                length
            );
        }
    }

    #[test]
    fn the_desktop_app_files_and_its_panel_session_live_in_their_own_folder() {
        let sock_dir = Path::new("/run/zellij/contract_version_1");
        let app = sock_dir.join(APP_DIR_NAME);
        assert_eq!(app_dir_in(sock_dir), app);
        assert_eq!(
            session_socket_path_in(sock_dir, APP_PANEL_SESSION),
            app.join(APP_PANEL_SESSION)
        );
        assert_eq!(
            session_socket_path_in(sock_dir, "api"),
            sock_dir.join("api")
        );
        for name in [
            APP_CONTROL_SOCKET_NAME,
            APP_ATTENTION_SOCKET_NAME,
            APP_LOCK_FILE_NAME,
        ] {
            assert!(!name.contains('/'));
        }
    }

    #[test]
    fn a_session_listing_that_keeps_only_sockets_never_sees_the_app_folder() {
        let dir = tempfile::tempdir().unwrap();
        prepare_private_dir(&app_dir_in(dir.path())).unwrap();
        let listed: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry
                    .file_type()
                    .map(|t| is_ipc_socket(&t))
                    .unwrap_or(false)
            })
            .collect();
        assert!(listed.is_empty());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(app_dir_in(dir.path()))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o700);
        }
    }

    #[cfg(unix)]
    #[test]
    fn pollhup_without_pollout_and_a_clear_so_error_is_connected() {
        // interprocess 2.3 `wait_for_connect`: POLLHUP/POLLERR with SO_ERROR == 0
        // falls through, then missing POLLOUT is a timeout. The probe maps that
        // timeout to Unknown and never reads Connected. macOS poll does this for
        // a Unix socket that has connected and is waiting in the accept queue.
        fn legacy_interprocess_wait(
            pollout: bool,
            pollhup: bool,
            pollerr: bool,
            so_error: i32,
        ) -> Result<(), &'static str> {
            if pollhup || pollerr {
                if so_error != 0 {
                    return Err("socket error");
                }
            }
            if !pollout {
                return Err("timed out");
            }
            Ok(())
        }

        let pollout = false;
        let pollhup = true;
        let pollerr = false;
        let so_error = 0;
        assert_eq!(
            legacy_interprocess_wait(pollout, pollhup, pollerr, so_error),
            Err("timed out")
        );
        assert_eq!(
            classify_unix_connect_readiness(UnixConnectPoll {
                pollout,
                pollhup,
                pollerr,
                pollnval: false,
                so_error,
            }),
            UnixConnectReadiness::Connected
        );
        assert_eq!(
            classify_unix_connect_readiness(UnixConnectPoll {
                pollout: true,
                pollhup: true,
                pollerr: false,
                pollnval: false,
                so_error: nix::errno::Errno::ECONNREFUSED as i32,
            }),
            UnixConnectReadiness::Failed(nix::errno::Errno::ECONNREFUSED as i32)
        );
        assert!(matches!(
            classify_unix_connect_readiness(UnixConnectPoll {
                pollout: false,
                pollhup: false,
                pollerr: false,
                pollnval: true,
                so_error: 0,
            }),
            UnixConnectReadiness::Failed(_)
        ));
    }
}

#[cfg(all(test, target_os = "linux"))]
mod timed_connect_backlog_regression {
    use super::ipc_connect_with_timeout;
    use std::io::ErrorKind;
    use std::os::unix::net::UnixListener;
    use std::time::{Duration, Instant};

    #[test]
    fn full_backlog_never_returns_an_unconnected_socket() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("full-backlog");
        let listener = UnixListener::bind(&path).unwrap();
        nix::sys::socket::listen(&listener, nix::sys::socket::Backlog::new(1).unwrap()).unwrap();
        // Linux admits backlog + 1 pending Unix stream connections.
        let _first = ipc_connect_with_timeout(&path, Duration::from_secs(1)).unwrap();
        let _second = ipc_connect_with_timeout(&path, Duration::from_secs(1)).unwrap();
        let started = Instant::now();
        let error = ipc_connect_with_timeout(&path, Duration::from_millis(80))
            .expect_err("EAGAIN plus POLLHUP/SO_ERROR=0 is not a connection");
        assert_eq!(error.kind(), ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(2));
        // Free a slot. A later attempt must still be able to connect.
        let (_accepted, _) = listener.accept().unwrap();
        let _third = ipc_connect_with_timeout(&path, Duration::from_secs(1))
            .expect("freed backlog capacity must be usable");
    }
}

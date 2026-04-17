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

#[cfg(not(windows))]
pub const SYSTEM_DEFAULT_CONFIG_DIR: &str = "/etc/zellij";
#[cfg(windows)]
pub const SYSTEM_DEFAULT_CONFIG_DIR: &str = "C:\\ProgramData\\Zellij";
pub const SYSTEM_DEFAULT_DATA_DIR_PREFIX: &str = system_default_data_dir();

pub static ZELLIJ_DEFAULT_THEMES: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/themes");

pub const CLIENT_SERVER_CONTRACT_VERSION: usize = 1;

pub fn session_info_cache_file_name(session_name: &str) -> PathBuf {
    session_info_folder_for_session(session_name).join("session-metadata.kdl")
}

pub fn session_layout_cache_file_name(session_name: &str) -> PathBuf {
    session_info_folder_for_session(session_name).join("session-layout.kdl")
}

pub fn session_info_folder_for_session(session_name: &str) -> PathBuf {
    ZELLIJ_SESSION_INFO_CACHE_DIR.join(session_name)
}

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
        if cfg!(windows) {
            ProjectDirs::from("", "", "Zellij").unwrap()
        } else {
            ProjectDirs::from("org", "Zellij Contributors", "Zellij").unwrap()
        }
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
    pub static ref ZELLIJ_STDIN_CACHE_FILE: PathBuf =
        ZELLIJ_CACHE_DIR.join(VERSION).join("stdin_cache");
    pub static ref ZELLIJ_PLUGIN_ARTIFACT_DIR: PathBuf = ZELLIJ_CACHE_DIR.join(VERSION);
    pub static ref ZELLIJ_SEEN_RELEASE_NOTES_CACHE_FILE: PathBuf =
        ZELLIJ_CACHE_DIR.join(VERSION).join("seen_release_notes");
}

pub const FEATURES: &[&str] = &[
    #[cfg(feature = "disable_automatic_asset_installation")]
    "disable_automatic_asset_installation",
];

#[cfg(not(target_family = "wasm"))]
pub use not_wasm::*;

#[cfg(not(target_family = "wasm"))]
mod not_wasm {
    use lazy_static::lazy_static;
    use std::collections::HashMap;
    use std::path::PathBuf;

    // Convenience macro to add plugins to the asset map (see `ASSET_MAP`)
    //
    // Plugins are taken from:
    //
    // - `zellij-utils/assets/plugins`: When building in release mode OR when the
    //   `plugins_from_target` feature IS NOT set
    // - `zellij-utils/../target/wasm32-wasip1/debug`: When building in debug mode AND the
    //   `plugins_from_target` feature IS set
    macro_rules! add_plugin {
        ($assets:expr, $plugin:literal) => {
            $assets.insert(
                PathBuf::from("plugins").join($plugin),
                #[cfg(any(not(feature = "plugins_from_target"), not(debug_assertions)))]
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/assets/plugins/",
                    $plugin
                ))
                .to_vec(),
                #[cfg(all(feature = "plugins_from_target", debug_assertions))]
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../target/wasm32-wasip1/debug/",
                    $plugin
                ))
                .to_vec(),
            );
        };
    }

    lazy_static! {
        // Zellij asset map
        pub static ref ASSET_MAP: HashMap<PathBuf, Vec<u8>> = {
            let mut assets = std::collections::HashMap::new();
            add_plugin!(assets, "compact-bar.wasm");
            add_plugin!(assets, "status-bar.wasm");
            add_plugin!(assets, "tab-bar.wasm");
            add_plugin!(assets, "strider.wasm");
            add_plugin!(assets, "session-manager.wasm");
            add_plugin!(assets, "configuration.wasm");
            add_plugin!(assets, "plugin-manager.wasm");
            add_plugin!(assets, "about.wasm");
            add_plugin!(assets, "share.wasm");
            add_plugin!(assets, "multiple-select.wasm");
            add_plugin!(assets, "layout-manager.wasm");
            add_plugin!(assets, "link.wasm");
            assets
        };
    }
}

/// Check if a filesystem entry is an IPC socket.
///
/// On Unix, this checks `FileTypeExt::is_socket()`. On non-Unix platforms,
/// this checks `is_file()` to detect marker files created by `ipc_bind()`
/// and `ipc_bind_async()` alongside kernel-level named pipes.
#[cfg(unix)]
pub fn is_ipc_socket(file_type: &std::fs::FileType) -> bool {
    use std::os::unix::fs::FileTypeExt;
    file_type.is_socket()
}

#[cfg(not(unix))]
pub fn is_ipc_socket(file_type: &std::fs::FileType) -> bool {
    file_type.is_file()
}

/// Connect to an IPC socket at the given path.
///
/// On Unix, this uses Unix domain sockets via `GenericFilePath`.
/// On Windows, this uses named pipes via `GenericNamespaced`.
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

/// Create an IPC listener bound to the given path.
///
/// On Unix, this uses Unix domain sockets via `GenericFilePath`.
/// On Windows, this uses named pipes via `GenericNamespaced` and creates
/// a marker file for session discovery.
#[cfg(unix)]
pub fn ipc_bind(path: &std::path::Path) -> std::io::Result<interprocess::local_socket::Listener> {
    use interprocess::local_socket::{prelude::*, GenericFilePath, ListenerOptions};
    let fs_name = path.to_fs_name::<GenericFilePath>()?;
    ListenerOptions::new().name(fs_name).create_sync()
}

#[cfg(windows)]
pub fn ipc_bind(path: &std::path::Path) -> std::io::Result<interprocess::local_socket::Listener> {
    use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions};
    use interprocess::os::windows::local_socket::ListenerOptionsExt;
    let name = path.to_string_lossy().to_string();
    let ns_name = name.to_ns_name::<GenericNamespaced>()?;
    // Set a security descriptor that grants access to the current user's SID
    // across all logon sessions. Without this, sessions created from SSH
    // cannot be attached to from an interactive desktop (or vice versa)
    // because the default pipe DACL only grants access to the creating
    // logon session's token.
    let mut opts = ListenerOptions::new().name(ns_name);
    if let Some(sd) = current_user_security_descriptor() {
        opts = opts.security_descriptor(sd);
    }
    let listener = opts.create_sync()?;
    std::fs::write(path, std::process::id().to_string())?;
    Ok(listener)
}

/// Create an async (tokio) IPC listener bound to the given path.
///
/// On Unix, this uses Unix domain sockets via `GenericFilePath`.
/// On Windows, this uses named pipes via `GenericNamespaced` and creates
/// a marker file for session discovery.
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
    use interprocess::os::windows::local_socket::ListenerOptionsExt;
    let name = path.to_string_lossy().to_string();
    let ns_name = name.to_ns_name::<GenericNamespaced>()?;
    let mut opts = ListenerOptions::new().name(ns_name);
    if let Some(sd) = current_user_security_descriptor() {
        opts = opts.security_descriptor(sd);
    }
    let listener = opts.create_tokio()?;
    std::fs::write(path, std::process::id().to_string())?;
    Ok(listener)
}

/// Connect to the reply pipe for a given IPC path (Windows only).
///
/// Uses `path-reply` as the named pipe for the server→client direction.
#[cfg(windows)]
pub fn ipc_connect_reply(
    path: &std::path::Path,
) -> std::io::Result<interprocess::local_socket::Stream> {
    use interprocess::local_socket::{prelude::*, GenericNamespaced, Stream as LocalSocketStream};
    let name = format!("{}-reply", path.to_string_lossy());
    let ns_name = name.to_ns_name::<GenericNamespaced>()?;
    LocalSocketStream::connect(ns_name)
}

/// Create an IPC listener for the reply pipe (Windows only).
///
/// Binds to `path-reply` as the named pipe for the server→client direction.
#[cfg(windows)]
pub fn ipc_bind_reply(
    path: &std::path::Path,
) -> std::io::Result<interprocess::local_socket::Listener> {
    use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions};
    use interprocess::os::windows::local_socket::ListenerOptionsExt;
    let name = format!("{}-reply", path.to_string_lossy());
    let ns_name = name.to_ns_name::<GenericNamespaced>()?;
    let mut opts = ListenerOptions::new().name(ns_name);
    if let Some(sd) = current_user_security_descriptor() {
        opts = opts.security_descriptor(sd);
    }
    opts.create_sync()
}

/// Build a security descriptor that grants full access to the current user's
/// SID. This allows named pipes to be accessed across different Windows logon
/// sessions (e.g., SSH vs interactive desktop) for the same user account,
/// without exposing them to other users.
///
/// Returns `None` if the user SID cannot be determined (falls back to default
/// pipe security).
#[cfg(windows)]
fn current_user_security_descriptor(
) -> Option<interprocess::os::windows::security_descriptor::SecurityDescriptor> {
    use interprocess::os::windows::security_descriptor::SecurityDescriptor;
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        // Get current process token.
        let mut token_handle = 0isize;
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token_handle) == 0 {
            return None;
        }

        // Query token for user SID.
        let mut token_info_len: u32 = 0;
        GetTokenInformation(
            token_handle,
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut token_info_len,
        );
        let mut token_info_buf = vec![0u8; token_info_len as usize];
        if GetTokenInformation(
            token_handle,
            TokenUser,
            token_info_buf.as_mut_ptr().cast(),
            token_info_len,
            &mut token_info_len,
        ) == 0
        {
            CloseHandle(token_handle);
            return None;
        }

        let token_user = &*(token_info_buf.as_ptr() as *const TOKEN_USER);
        let sid = token_user.User.Sid;

        // Convert SID to string (e.g., "S-1-5-21-...").
        let mut sid_string_ptr: *mut u16 = std::ptr::null_mut();
        if ConvertSidToStringSidW(sid, &mut sid_string_ptr) == 0 {
            CloseHandle(token_handle);
            return None;
        }

        // Build SDDL: grant Generic All to this specific user SID.
        let sid_string = widestring::U16CStr::from_ptr_str(sid_string_ptr);
        let sddl_str = format!("D:(A;;GA;;;{})", sid_string.to_string_lossy());
        let sddl_wide = widestring::U16CString::from_str(&sddl_str).ok();

        LocalFree(sid_string_ptr as *mut std::ffi::c_void);
        CloseHandle(token_handle);

        sddl_wide.and_then(|s| SecurityDescriptor::deserialize(&s).ok())
    }
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

    // Maximum length of a Unix domain socket path (from sockaddr_un.sun_path).
    // macOS (and other BSDs) use 104, Linux/Android/Solaris use 108.
    // The not(target_os = "macos") fallback of 108 is used for all other Unix
    // platforms — this is correct for Linux/Android/Solaris and only 4 bytes
    // over for BSDs, which would cause a slightly late error rather than a
    // missed one.
    #[cfg(target_os = "macos")]
    pub const ZELLIJ_SOCK_MAX_LENGTH: usize = 104;
    #[cfg(not(target_os = "macos"))]
    pub const ZELLIJ_SOCK_MAX_LENGTH: usize = 108;

    lazy_static! {
        static ref UID: Uid = Uid::current();
        pub static ref ZELLIJ_TMP_DIR: PathBuf = temp_dir().join(format!("zellij-{}", *UID));
        pub static ref ZELLIJ_TMP_LOG_DIR: PathBuf = ZELLIJ_TMP_DIR.join("zellij-log");
        pub static ref ZELLIJ_TMP_LOG_FILE: PathBuf = ZELLIJ_TMP_LOG_DIR.join("zellij.log");
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
            tmp_dir.join("zellij")
        };
        pub static ref ZELLIJ_TMP_LOG_DIR: PathBuf = ZELLIJ_TMP_DIR.join("zellij-log");
        pub static ref ZELLIJ_TMP_LOG_FILE: PathBuf = ZELLIJ_TMP_LOG_DIR.join("zellij.log");
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

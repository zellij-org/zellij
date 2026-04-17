use crate::{
    consts::{
        is_ipc_socket, session_info_folder_for_session, session_layout_cache_file_name,
        ZELLIJ_SESSIONS_KDL, ZELLIJ_SESSIONS_LOCK, ZELLIJ_SESSION_INFO_CACHE_DIR, ZELLIJ_SOCK_DIR,
    },
    envs,
    input::layout::Layout,
    ipc::{ClientToServerMsg, IpcReceiverWithContext, IpcSenderWithContext, ServerToClientMsg},
};
use anyhow;
use humantime::format_duration;
use kdl::{KdlDocument, KdlNode, KdlValue};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use std::{fs, io, process};
use suggest::Suggest;
use uuid::Uuid;

/// A single session entry in the registry.
#[derive(Debug, Clone)]
pub struct SessionEntry {
    /// UUID v4 identifier (also the socket/marker filename).
    pub id: String,
    /// User-visible session name.
    pub display_name: String,
    /// Server PID (only meaningful while state == Running).
    pub pid: Option<u32>,
    /// Running or exited.
    pub state: SessionState,
    /// When the session was created.
    pub created_at: String,
    /// When the session exited (only for Exited state).
    pub exited_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    Running,
    Exited,
}

impl SessionState {
    pub fn as_str(&self) -> &str {
        match self {
            SessionState::Running => "running",
            SessionState::Exited => "exited",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "running" => Some(SessionState::Running),
            "exited" => Some(SessionState::Exited),
            _ => None,
        }
    }
}

/// The full session registry.
#[derive(Debug, Clone, Default)]
pub struct SessionRegistry {
    pub sessions: Vec<SessionEntry>,
}

/// Generate a new UUID v4 session identifier.
pub fn generate_session_id() -> String {
    Uuid::new_v4().as_hyphenated().to_string()
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
        }
    }

    /// Parse a `sessions.kdl` string into a registry.
    pub fn from_kdl(raw: &str) -> Result<Self, String> {
        let doc: KdlDocument = raw
            .parse()
            .map_err(|e| format!("Failed to parse sessions.kdl: {}", e))?;
        let mut sessions = Vec::new();
        for node in doc.nodes() {
            if node.name().value() != "session" {
                continue;
            }
            let id = node
                .entries()
                .iter()
                .find(|e| e.name().is_none())
                .and_then(|e| e.value().as_string())
                .unwrap_or("")
                .to_string();
            if id.is_empty() {
                continue;
            }
            let children = match node.children() {
                Some(c) => c,
                None => continue,
            };
            let display_name = children
                .get("display_name")
                .and_then(|n| n.entries().iter().next())
                .and_then(|e| e.value().as_string())
                .unwrap_or("")
                .to_string();
            let pid = children
                .get("pid")
                .and_then(|n| n.entries().iter().next())
                .and_then(|e| e.value().as_i64())
                .map(|v| v as u32);
            let state_str = children
                .get("state")
                .and_then(|n| n.entries().iter().next())
                .and_then(|e| e.value().as_string())
                .unwrap_or("running");
            let state = SessionState::from_str(state_str).unwrap_or(SessionState::Running);
            let created_at = children
                .get("created_at")
                .and_then(|n| n.entries().iter().next())
                .and_then(|e| e.value().as_string())
                .unwrap_or("")
                .to_string();
            let exited_at = children
                .get("exited_at")
                .and_then(|n| n.entries().iter().next())
                .and_then(|e| e.value().as_string())
                .map(|s| s.to_string());

            sessions.push(SessionEntry {
                id,
                display_name,
                pid,
                state,
                created_at,
                exited_at,
            });
        }
        Ok(SessionRegistry { sessions })
    }

    /// Serialize the registry to a KDL string.
    pub fn to_kdl(&self) -> String {
        let mut doc = KdlDocument::new();
        for entry in &self.sessions {
            let mut node = KdlNode::new("session");
            node.push(KdlValue::String(entry.id.clone()));

            let mut children = KdlDocument::new();

            let mut dn = KdlNode::new("display_name");
            dn.push(KdlValue::String(entry.display_name.clone()));
            children.nodes_mut().push(dn);

            if let Some(pid) = entry.pid {
                let mut pn = KdlNode::new("pid");
                pn.push(KdlValue::Base10(pid as i64));
                children.nodes_mut().push(pn);
            }

            let mut sn = KdlNode::new("state");
            sn.push(KdlValue::String(entry.state.as_str().to_string()));
            children.nodes_mut().push(sn);

            if !entry.created_at.is_empty() {
                let mut cn = KdlNode::new("created_at");
                cn.push(KdlValue::String(entry.created_at.clone()));
                children.nodes_mut().push(cn);
            }

            if let Some(ref exited_at) = entry.exited_at {
                let mut en = KdlNode::new("exited_at");
                en.push(KdlValue::String(exited_at.clone()));
                children.nodes_mut().push(en);
            }

            node.set_children(children);
            doc.nodes_mut().push(node);
        }
        doc.fmt();
        doc.to_string()
    }

    /// Find a running session by display name.
    pub fn find_running_by_name(&self, name: &str) -> Option<&SessionEntry> {
        self.sessions
            .iter()
            .find(|s| s.display_name == name && s.state == SessionState::Running)
    }

    /// Find a session (any state) by display name.
    pub fn find_by_name(&self, name: &str) -> Option<&SessionEntry> {
        self.sessions.iter().find(|s| s.display_name == name)
    }

    /// Find a session by UUID.
    pub fn find_by_id(&self, id: &str) -> Option<&SessionEntry> {
        self.sessions.iter().find(|s| s.id == id)
    }

    /// Find a mutable session by UUID.
    pub fn find_by_id_mut(&mut self, id: &str) -> Option<&mut SessionEntry> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    /// Get all running sessions.
    pub fn running_sessions(&self) -> Vec<&SessionEntry> {
        self.sessions
            .iter()
            .filter(|s| s.state == SessionState::Running)
            .collect()
    }

    /// Get all exited sessions.
    pub fn exited_sessions(&self) -> Vec<&SessionEntry> {
        self.sessions
            .iter()
            .filter(|s| s.state == SessionState::Exited)
            .collect()
    }

    /// Remove a session by UUID.
    pub fn remove_by_id(&mut self, id: &str) {
        self.sessions.retain(|s| s.id != id);
    }

    /// Resolve a session display name to a socket path.
    pub fn resolve_socket_path(&self, name: &str) -> Option<PathBuf> {
        self.find_running_by_name(name)
            .map(|entry| ZELLIJ_SOCK_DIR.join(&entry.id))
    }
}

#[cfg(unix)]
mod file_lock {
    use std::fs::{File, OpenOptions};
    use std::os::unix::io::AsRawFd;
    use std::path::Path;

    pub struct FileLock {
        _file: File,
    }

    impl FileLock {
        pub fn exclusive(path: &Path) -> std::io::Result<Self> {
            let file = OpenOptions::new().create(true).write(true).open(path)?;
            let fd = file.as_raw_fd();
            let ret = unsafe { libc::flock(fd, libc::LOCK_EX) };
            if ret != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(FileLock { _file: file })
        }
    }

    // Lock is released when _file is dropped (fd closed → flock released).
}

#[cfg(windows)]
mod file_lock {
    use std::fs::{File, OpenOptions};
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;

    pub struct FileLock {
        _file: File,
    }

    impl FileLock {
        pub fn exclusive(path: &Path) -> std::io::Result<Self> {
            let file = OpenOptions::new().create(true).write(true).open(path)?;
            let handle = file.as_raw_handle();
            unsafe {
                use windows_sys::Win32::Foundation::HANDLE;
                use windows_sys::Win32::Storage::FileSystem::{
                    LockFileEx, LOCKFILE_EXCLUSIVE_LOCK,
                };
                let mut overlapped: windows_sys::Win32::System::IO::OVERLAPPED = std::mem::zeroed();
                let ret = LockFileEx(
                    handle as HANDLE,
                    LOCKFILE_EXCLUSIVE_LOCK,
                    0,
                    u32::MAX,
                    u32::MAX,
                    &mut overlapped,
                );
                if ret == 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(FileLock { _file: file })
        }
    }

    // Lock is released when _file is dropped (handle closed → lock released).
}

#[cfg(not(any(unix, windows)))]
mod file_lock {
    use std::fs::{File, OpenOptions};
    use std::path::Path;

    pub struct FileLock {
        _file: File,
    }

    impl FileLock {
        pub fn exclusive(path: &Path) -> std::io::Result<Self> {
            let file = OpenOptions::new().create(true).write(true).open(path)?;
            Ok(FileLock { _file: file })
        }
    }
}

use file_lock::FileLock;

/// Returns true if the session registry file exists on disk.
pub fn registry_exists() -> bool {
    ZELLIJ_SESSIONS_KDL.exists()
}

/// Read the session registry from disk, creating an empty one if it doesn't exist.
pub fn read_registry() -> SessionRegistry {
    match fs::read_to_string(&*ZELLIJ_SESSIONS_KDL) {
        Ok(raw) => match SessionRegistry::from_kdl(&raw) {
            Ok(reg) => reg,
            Err(e) => {
                log::error!("{}", e);
                SessionRegistry::new()
            },
        },
        Err(_) => SessionRegistry::new(),
    }
}

/// Migrate legacy sessions (pre-registry) into a new `sessions.kdl`.
///
/// Scans `ZELLIJ_SOCK_DIR` for old-format socket/marker files (named by
/// session name), creates registry entries for them, and writes the file.
/// Called once when `sessions.kdl` doesn't exist.
pub fn migrate_legacy_sessions() -> SessionRegistry {
    let mut registry = SessionRegistry::new();

    // Migrate live sessions from socket/marker files.
    if let Ok(files) = fs::read_dir(&*ZELLIJ_SOCK_DIR) {
        for file in files.flatten() {
            let file_name = match file.file_name().into_string() {
                Ok(n) => n,
                Err(_) => continue,
            };
            if file_name == "sessions.kdl" || file_name == "sessions.kdl.lock" {
                continue;
            }
            let file_type = match file.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if !is_ipc_socket(&file_type) {
                continue;
            }
            if !assert_socket(&file_name) {
                continue;
            }
            // This is a live legacy session — the filename IS the session name.
            // We keep the original file as-is (don't rename), so the "id" is
            // the old session name (not a UUID). This allows ipc_connect to
            // still find it by joining ZELLIJ_SOCK_DIR with the id.
            let ctime = std::fs::metadata(file.path())
                .ok()
                .and_then(|f| f.created().ok().or_else(|| f.modified().ok()))
                .and_then(|d| d.elapsed().ok())
                .unwrap_or_default();
            let created_at = {
                let secs_ago = ctime.as_secs();
                let now = chrono::Utc::now();
                let then = now - chrono::Duration::seconds(secs_ago as i64);
                then.format("%Y-%m-%dT%H:%M:%SZ").to_string()
            };

            #[cfg(windows)]
            let pid = {
                // Read PID from marker file content (first line).
                fs::read_to_string(file.path())
                    .ok()
                    .and_then(|s| s.lines().next().and_then(|l| l.trim().parse::<u32>().ok()))
            };
            #[cfg(not(windows))]
            let pid: Option<u32> = None;

            registry.sessions.push(SessionEntry {
                id: file_name.clone(),
                display_name: file_name,
                pid,
                state: SessionState::Running,
                created_at,
                exited_at: None,
            });
        }
    }

    // Migrate resurrectable sessions from the session_info cache.
    if let Ok(dirs) = fs::read_dir(&*ZELLIJ_SESSION_INFO_CACHE_DIR) {
        for dir in dirs.flatten() {
            let path = dir.path();
            if !path.is_dir() {
                continue;
            }
            let session_name = match path.file_name().and_then(|f| f.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            // Skip if already migrated as a running session.
            if registry.find_by_name(&session_name).is_some() {
                continue;
            }
            let layout_file = session_layout_cache_file_name(&session_name);
            if !std::path::Path::new(&layout_file).exists() {
                continue;
            }
            let ctime = std::fs::metadata(&layout_file)
                .ok()
                .and_then(|m| m.created().ok().or_else(|| m.modified().ok()))
                .and_then(|d| d.elapsed().ok())
                .unwrap_or_default();
            let created_at = {
                let secs_ago = ctime.as_secs();
                let now = chrono::Utc::now();
                let then = now - chrono::Duration::seconds(secs_ago as i64);
                then.format("%Y-%m-%dT%H:%M:%SZ").to_string()
            };
            registry.sessions.push(SessionEntry {
                id: generate_session_id(),
                display_name: session_name,
                pid: None,
                state: SessionState::Exited,
                exited_at: Some(created_at.clone()),
                created_at,
            });
        }
    }

    // Write the newly created registry.
    if let Err(e) = write_registry(&registry) {
        log::error!("Failed to write migrated session registry: {:?}", e);
    }

    registry
}

/// Ensure the session registry exists. If not, migrate from legacy format.
/// Returns the current registry.
pub fn ensure_registry() -> SessionRegistry {
    if registry_exists() {
        read_registry()
    } else {
        migrate_legacy_sessions()
    }
}

/// Write the session registry to disk, acquiring an exclusive lock.
pub fn write_registry(registry: &SessionRegistry) -> io::Result<()> {
    let _lock = FileLock::exclusive(&ZELLIJ_SESSIONS_LOCK)?;
    fs::write(&*ZELLIJ_SESSIONS_KDL, registry.to_kdl())
}

/// Read the registry under an exclusive lock, apply a mutation, and write it back.
/// Returns the return value of the closure.
pub fn with_registry<F, R>(f: F) -> io::Result<R>
where
    F: FnOnce(&mut SessionRegistry) -> R,
{
    let _lock = FileLock::exclusive(&ZELLIJ_SESSIONS_LOCK)?;
    let mut registry = match fs::read_to_string(&*ZELLIJ_SESSIONS_KDL) {
        Ok(raw) => SessionRegistry::from_kdl(&raw).unwrap_or_default(),
        Err(_) => SessionRegistry::new(),
    };
    let result = f(&mut registry);
    fs::write(&*ZELLIJ_SESSIONS_KDL, registry.to_kdl())?;
    Ok(result)
}

/// Register a new session in the registry. Returns the generated UUID.
pub fn register_session(display_name: &str) -> io::Result<String> {
    let id = generate_session_id();
    let entry = SessionEntry {
        id: id.clone(),
        display_name: display_name.to_string(),
        pid: None,
        state: SessionState::Running,
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        exited_at: None,
    };
    with_registry(|reg| {
        reg.sessions.push(entry);
    })?;
    Ok(id)
}

/// Resolve a session display name to its socket path via the registry.
pub fn resolve_session_socket_path(name: &str) -> Option<PathBuf> {
    ensure_registry().resolve_socket_path(name)
}

pub fn get_sessions() -> Result<Vec<(String, Duration)>, io::ErrorKind> {
    let registry = ensure_registry();
    let mut sessions = Vec::new();
    for entry in registry.running_sessions() {
        let sock_path = ZELLIJ_SOCK_DIR.join(&entry.id);
        let ctime = std::fs::metadata(&sock_path)
            .ok()
            .and_then(|f| f.created().ok().or_else(|| f.modified().ok()))
            .and_then(|d| d.elapsed().ok())
            .unwrap_or_default();
        let duration = Duration::from_secs(ctime.as_secs());
        if assert_socket(&entry.id) {
            sessions.push((entry.display_name.clone(), duration));
        }
    }
    Ok(sessions)
}

pub fn get_resurrectable_sessions() -> Vec<(String, Duration)> {
    match fs::read_dir(&*ZELLIJ_SESSION_INFO_CACHE_DIR) {
        Ok(files_in_session_info_folder) => {
            let files_that_are_folders = files_in_session_info_folder
                .filter_map(|f| f.ok().map(|f| f.path()))
                .filter(|f| f.is_dir());
            files_that_are_folders
                .filter_map(|folder_name| {
                    let layout_file_name =
                        session_layout_cache_file_name(&folder_name.display().to_string());
                    // Try to get creation time, fall back to modification time on platforms where it's not supported (e.g., musl)
                    let ctime = std::fs::metadata(&layout_file_name)
                        .ok()
                        .and_then(|metadata| {
                            metadata.created().ok().or_else(|| metadata.modified().ok())
                        });
                    let elapsed_duration = ctime
                        .map(|ctime| {
                            Duration::from_secs(ctime.elapsed().ok().unwrap_or_default().as_secs())
                        })
                        .unwrap_or_default();
                    let session_name = folder_name
                        .file_name()
                        .map(|f| std::path::PathBuf::from(f).display().to_string())?;
                    if std::path::Path::new(&layout_file_name).exists() {
                        Some((session_name, elapsed_duration))
                    } else {
                        None
                    }
                })
                .collect()
        },
        Err(e) => {
            log::error!(
                "Failed to read session_info cache folder: \"{:?}\": {:?}",
                &*ZELLIJ_SESSION_INFO_CACHE_DIR,
                e
            );
            vec![]
        },
    }
}

pub fn get_resurrectable_session_names() -> Vec<String> {
    match fs::read_dir(&*ZELLIJ_SESSION_INFO_CACHE_DIR) {
        Ok(files_in_session_info_folder) => {
            let files_that_are_folders = files_in_session_info_folder
                .filter_map(|f| f.ok().map(|f| f.path()))
                .filter(|f| f.is_dir());
            files_that_are_folders
                .filter_map(|folder_name| {
                    let folder = folder_name.display().to_string();
                    let resurrection_layout_file = session_layout_cache_file_name(&folder);
                    if std::path::Path::new(&resurrection_layout_file).exists() {
                        folder_name
                            .file_name()
                            .map(|f| format!("{}", f.to_string_lossy()))
                    } else {
                        None
                    }
                })
                .collect()
        },
        Err(e) => {
            log::error!(
                "Failed to read session_info cache folder: \"{:?}\": {:?}",
                &*ZELLIJ_SESSION_INFO_CACHE_DIR,
                e
            );
            vec![]
        },
    }
}

pub fn get_sessions_sorted_by_mtime() -> anyhow::Result<Vec<String>> {
    let registry = ensure_registry();
    let mut sessions_with_mtime: Vec<(String, SystemTime)> = Vec::new();
    for entry in registry.running_sessions() {
        let sock_path = ZELLIJ_SOCK_DIR.join(&entry.id);
        if let Ok(meta) = std::fs::metadata(&sock_path) {
            if let Ok(mtime) = meta.modified() {
                if assert_socket(&entry.id) {
                    sessions_with_mtime.push((entry.display_name.clone(), mtime));
                }
            }
        }
    }
    sessions_with_mtime.sort_by_key(|x| x.1);
    Ok(sessions_with_mtime.into_iter().map(|x| x.0).collect())
}

/// Probe a session socket to check if a server is alive.
///
/// On Unix, connects and sends a `ConnStatus` message to verify the server responds.
/// On Windows, reads the server PID from the marker file and checks process liveness.
#[cfg(unix)]
fn assert_socket(name: &str) -> bool {
    use crate::consts::ipc_connect;
    let path = &*ZELLIJ_SOCK_DIR.join(name);
    match ipc_connect(path) {
        Ok(stream) => {
            let mut sender: IpcSenderWithContext<ClientToServerMsg> =
                IpcSenderWithContext::new(stream);
            let _ = sender.send_client_msg(ClientToServerMsg::ConnStatus);
            let mut receiver: IpcReceiverWithContext<ServerToClientMsg> = sender.get_receiver();
            match receiver.recv_server_msg() {
                Some((ServerToClientMsg::Connected, _)) => true,
                None | Some((_, _)) => false,
            }
        },
        Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
            drop(fs::remove_file(path));
            false
        },
        Err(_) => false,
    }
}

/// On Windows, reads the server PID from the marker file and checks whether
/// the process is still alive via `OpenProcess`. Cleans up stale marker files.
#[cfg(windows)]
fn assert_socket(name: &str) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    let path = &*ZELLIJ_SOCK_DIR.join(name);
    let pid_str = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => {
            drop(fs::remove_file(path));
            return false;
        },
    };
    let pid: u32 = match pid_str.trim().parse() {
        Ok(p) => p,
        Err(_) => {
            // Marker file exists but has no valid PID (e.g. empty from old version).
            // Treat as stale.
            drop(fs::remove_file(path));
            return false;
        },
    };
    let alive = unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle == 0 {
            false
        } else {
            CloseHandle(handle);
            true
        }
    };
    if !alive {
        drop(fs::remove_file(path));
    }
    alive
}

#[cfg(not(any(unix, windows)))]
fn assert_socket(_name: &str) -> bool {
    true
}

pub fn print_sessions(
    mut sessions: Vec<(String, Duration, bool)>,
    no_formatting: bool,
    short: bool,
    reverse: bool,
) {
    // (session_name, timestamp, is_dead)
    let curr_session = envs::get_session_name().unwrap_or_else(|_| "".into());
    sessions.sort_by(|a, b| {
        if reverse {
            // sort by `Duration` ascending (newest would be first)
            a.1.cmp(&b.1)
        } else {
            b.1.cmp(&a.1)
        }
    });
    sessions
        .iter()
        .for_each(|(session_name, timestamp, is_dead)| {
            if short {
                println!("{}", session_name);
                return;
            }
            if no_formatting {
                let suffix = if curr_session == *session_name {
                    format!("(current)")
                } else if *is_dead {
                    format!("(EXITED - attach to resurrect)")
                } else {
                    String::new()
                };
                let timestamp = format!("[Created {} ago]", format_duration(*timestamp));
                println!("{} {} {}", session_name, timestamp, suffix);
            } else {
                let formatted_session_name = format!("\u{1b}[32;1m{}\u{1b}[m", session_name);
                let suffix = if curr_session == *session_name {
                    format!("(current)")
                } else if *is_dead {
                    format!("(\u{1b}[31;1mEXITED\u{1b}[m - attach to resurrect)")
                } else {
                    String::new()
                };
                let timestamp = format!(
                    "[Created \u{1b}[35;1m{}\u{1b}[m ago]",
                    format_duration(*timestamp)
                );
                println!("{} {} {}", formatted_session_name, timestamp, suffix);
            }
        })
}

pub fn print_sessions_with_index(sessions: Vec<String>) {
    let curr_session = envs::get_session_name().unwrap_or_else(|_| "".into());
    for (i, session) in sessions.iter().enumerate() {
        let suffix = if curr_session == *session {
            " (current)"
        } else {
            ""
        };
        println!("{}: {}{}", i, session, suffix);
    }
}

pub enum ActiveSession {
    None,
    One(String),
    Many,
}

pub fn get_active_session() -> ActiveSession {
    match get_sessions() {
        Ok(sessions) if sessions.is_empty() => ActiveSession::None,
        Ok(mut sessions) if sessions.len() == 1 => ActiveSession::One(sessions.pop().unwrap().0),
        Ok(_) => ActiveSession::Many,
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
            process::exit(1);
        },
    }
}

pub fn kill_session(name: &str) {
    use crate::consts::ipc_connect;
    let resolved = resolve_session_socket_path(name).unwrap_or_else(|| ZELLIJ_SOCK_DIR.join(name));
    let path = &*resolved;
    match ipc_connect(path) {
        Ok(stream) => {
            // On Windows, the server uses a dual-pipe architecture: the main pipe
            // for client→server and a reply pipe for server→client. We must:
            // 1. Connect to the reply pipe (so the server unblocks from
            //    reply_listener.accept() and spawns the route thread)
            // 2. Send KillSession on the main pipe
            // 3. Wait for the Exit response on the reply pipe (so we don't
            //    disconnect before the server processes the message)
            #[cfg(windows)]
            {
                let reply = crate::consts::ipc_connect_reply(path);
                let _ = IpcSenderWithContext::<ClientToServerMsg>::new(stream)
                    .send_client_msg(ClientToServerMsg::KillSession);
                if let Ok(reply_stream) = reply {
                    let mut receiver: IpcReceiverWithContext<ServerToClientMsg> =
                        IpcReceiverWithContext::new(reply_stream);
                    let _ = receiver.recv_server_msg();
                }
            }
            #[cfg(not(windows))]
            {
                let _ = IpcSenderWithContext::<ClientToServerMsg>::new(stream)
                    .send_client_msg(ClientToServerMsg::KillSession);
            }
        },
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
            process::exit(1);
        },
    };
}

pub fn delete_session(name: &str, force: bool) {
    if force {
        use crate::consts::ipc_connect;
        let resolved =
            resolve_session_socket_path(name).unwrap_or_else(|| ZELLIJ_SOCK_DIR.join(name));
        let path = &*resolved;
        let _ = ipc_connect(path).ok().map(|stream| {
            #[cfg(windows)]
            {
                let reply = crate::consts::ipc_connect_reply(path);
                let _ = IpcSenderWithContext::<ClientToServerMsg>::new(stream)
                    .send_client_msg(ClientToServerMsg::KillSession);
                if let Ok(reply_stream) = reply {
                    let mut receiver: IpcReceiverWithContext<ServerToClientMsg> =
                        IpcReceiverWithContext::new(reply_stream);
                    let _ = receiver.recv_server_msg();
                }
            }
            #[cfg(not(windows))]
            {
                IpcSenderWithContext::<ClientToServerMsg>::new(stream)
                    .send_client_msg(ClientToServerMsg::KillSession)
                    .ok();
            }
        });
    }
    if let Err(e) = std::fs::remove_dir_all(session_info_folder_for_session(name)) {
        if e.kind() == std::io::ErrorKind::NotFound {
            eprintln!("Session: {:?} not found.", name);
            process::exit(2);
        } else {
            log::error!("Failed to remove session {:?}: {:?}", name, e);
        }
    } else {
        println!("Session: {:?} successfully deleted.", name);
    }
}

pub fn list_sessions(no_formatting: bool, short: bool, reverse: bool) {
    let exit_code = match get_sessions() {
        Ok(running_sessions) => {
            let resurrectable_sessions = get_resurrectable_sessions();
            let mut all_sessions: HashMap<String, (Duration, bool)> = resurrectable_sessions
                .iter()
                .map(|(name, timestamp)| (name.clone(), (timestamp.clone(), true)))
                .collect();
            for (session_name, duration) in running_sessions {
                all_sessions.insert(session_name.clone(), (duration, false));
            }
            if all_sessions.is_empty() {
                eprintln!("No active zellij sessions found.");
                1
            } else {
                print_sessions(
                    all_sessions
                        .iter()
                        .map(|(name, (timestamp, is_dead))| {
                            (name.clone(), timestamp.clone(), *is_dead)
                        })
                        .collect(),
                    no_formatting,
                    short,
                    reverse,
                );
                0
            }
        },
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
            1
        },
    };
    process::exit(exit_code);
}

#[derive(Debug, Clone)]
pub enum SessionNameMatch {
    AmbiguousPrefix(Vec<String>),
    UniquePrefix(String),
    Exact(String),
    None,
}

pub fn match_session_name(prefix: &str) -> Result<SessionNameMatch, io::ErrorKind> {
    let sessions = get_sessions()?;

    let filtered_sessions: Vec<_> = sessions
        .iter()
        .filter(|s| s.0.starts_with(prefix))
        .collect();

    if filtered_sessions.iter().any(|s| s.0 == prefix) {
        return Ok(SessionNameMatch::Exact(prefix.to_string()));
    }

    Ok({
        match &filtered_sessions[..] {
            [] => SessionNameMatch::None,
            [s] => SessionNameMatch::UniquePrefix(s.0.to_string()),
            _ => SessionNameMatch::AmbiguousPrefix(
                filtered_sessions.into_iter().map(|s| s.0.clone()).collect(),
            ),
        }
    })
}

pub fn session_exists(name: &str) -> Result<bool, io::ErrorKind> {
    match match_session_name(name) {
        Ok(SessionNameMatch::Exact(_)) => Ok(true),
        Ok(_) => Ok(false),
        Err(e) => Err(e),
    }
}

// if the session is resurrecable, the returned layout is the one to be used to resurrect it
pub fn resurrection_layout(session_name_to_resurrect: &str) -> Result<Option<Layout>, String> {
    let layout_file_name = session_layout_cache_file_name(&session_name_to_resurrect);
    let raw_layout = match std::fs::read_to_string(&layout_file_name) {
        Ok(raw_layout) => raw_layout,
        Err(_e) => {
            return Ok(None);
        },
    };
    match Layout::from_kdl(
        &raw_layout,
        Some(layout_file_name.display().to_string()),
        None,
        None,
    ) {
        Ok(layout) => Ok(Some(layout)),
        Err(e) => {
            log::error!(
                "Failed to parse resurrection layout file {}: {}",
                layout_file_name.display(),
                e
            );
            return Err(format!(
                "Failed to parse resurrection layout file {}: {}.",
                layout_file_name.display(),
                e
            ));
        },
    }
}

pub fn assert_session(name: &str) {
    match session_exists(name) {
        Ok(result) => {
            if result {
                return;
            } else {
                println!("No session named {:?} found.", name);
                if let Some(sugg) = get_sessions()
                    .unwrap()
                    .iter()
                    .map(|s| s.0.clone())
                    .collect::<Vec<_>>()
                    .suggest(name)
                {
                    println!("  help: Did you mean `{}`?", sugg);
                }
            }
        },
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
        },
    };
    process::exit(1);
}

pub fn assert_dead_session(name: &str, force: bool) {
    match session_exists(name) {
        Ok(exists) => {
            if exists && !force {
                println!(
                    "A session by the name {:?} exists and is active, use --force to delete it.",
                    name
                )
            } else if exists && force {
                println!("A session by the name {:?} exists and is active, but will be force killed and deleted.", name);
                return;
            } else {
                return;
            }
        },
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
        },
    };
    process::exit(1);
}

pub fn validate_session_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err(
            "Session name cannot be empty. Please provide a specific session name.".to_string(),
        );
    }
    if name == "." || name == ".." {
        return Err(format!("Invalid session name: \"{}\".", name));
    }
    if name.contains('/') {
        return Err("Session name cannot contain '/'.".to_string());
    }
    Ok(())
}

pub fn assert_session_ne(name: &str) {
    if let Err(e) = validate_session_name(name) {
        eprintln!("{}", e);
        process::exit(1);
    }

    match session_exists(name) {
        Ok(result) if !result => {
            let resurrectable_sessions = get_resurrectable_session_names();
            if resurrectable_sessions.iter().find(|s| s == &name).is_some() {
                println!("Session with name {:?} already exists, but is dead. Use the attach command to resurrect it or, the delete-session command to kill it or specify a different name.", name);
            } else {
                return
            }
        }
        Ok(_) => println!("Session with name {:?} already exists. Use attach command to connect to it or specify a different name.", name),
        Err(e) => eprintln!("Error occurred: {:?}", e),
    };
    process::exit(1);
}

pub fn generate_unique_session_name() -> Option<String> {
    let sessions = get_sessions().map(|sessions| {
        sessions
            .iter()
            .map(|s| s.0.clone())
            .collect::<Vec<String>>()
    });
    let dead_sessions = get_resurrectable_session_names();
    let Ok(sessions) = sessions else {
        eprintln!("Failed to list existing sessions: {:?}", sessions);
        return None;
    };

    let name = get_name_generator()
        .take(1000)
        .find(|name| !sessions.contains(name) && !dead_sessions.contains(name));

    if let Some(name) = name {
        return Some(name);
    } else {
        return None;
    }
}

/// Create a new random name generator
///
/// Used to provide a memorable handle for a session when users don't specify a session name when the session is
/// created.
///
/// Uses the list of adjectives and nouns defined below, with the intention of avoiding unfortunate
/// and offensive combinations. Care should be taken when adding or removing to either list due to the birthday paradox/
/// hash collisions, e.g. with 4096 unique names, the likelihood of a collision in 10 session names is 1%.
pub fn get_name_generator() -> impl Iterator<Item = String> {
    names::Generator::new(&ADJECTIVES, &NOUNS, names::Name::Plain)
}

/// Generates a random human-readable name using curated adjectives and nouns.
/// Returns a single name in the format: AdjectiveNoun (e.g., "BraveRustacean")
pub fn generate_random_name() -> String {
    get_name_generator().next().unwrap()
}

const ADJECTIVES: &[&'static str] = &[
    "adamant",
    "adept",
    "adventurous",
    "arcadian",
    "auspicious",
    "awesome",
    "blossoming",
    "brave",
    "charming",
    "chatty",
    "circular",
    "considerate",
    "cubic",
    "curious",
    "delighted",
    "didactic",
    "diligent",
    "effulgent",
    "erudite",
    "excellent",
    "exquisite",
    "fabulous",
    "fascinating",
    "friendly",
    "glowing",
    "gracious",
    "gregarious",
    "hopeful",
    "implacable",
    "inventive",
    "joyous",
    "judicious",
    "jumping",
    "kind",
    "likable",
    "loyal",
    "lucky",
    "marvellous",
    "mellifluous",
    "nautical",
    "oblong",
    "outstanding",
    "polished",
    "polite",
    "profound",
    "quadratic",
    "quiet",
    "rectangular",
    "remarkable",
    "rusty",
    "sensible",
    "sincere",
    "sparkling",
    "splendid",
    "stellar",
    "tenacious",
    "tremendous",
    "triangular",
    "undulating",
    "unflappable",
    "unique",
    "verdant",
    "vitreous",
    "wise",
    "zippy",
];

const NOUNS: &[&'static str] = &[
    "aardvark",
    "accordion",
    "apple",
    "apricot",
    "bee",
    "brachiosaur",
    "cactus",
    "capsicum",
    "clarinet",
    "cowbell",
    "crab",
    "cuckoo",
    "cymbal",
    "diplodocus",
    "donkey",
    "drum",
    "duck",
    "echidna",
    "elephant",
    "foxglove",
    "galaxy",
    "glockenspiel",
    "goose",
    "hill",
    "horse",
    "iguanadon",
    "jellyfish",
    "kangaroo",
    "lake",
    "lemon",
    "lemur",
    "magpie",
    "megalodon",
    "mountain",
    "mouse",
    "muskrat",
    "newt",
    "oboe",
    "ocelot",
    "orange",
    "panda",
    "peach",
    "pepper",
    "petunia",
    "pheasant",
    "piano",
    "pigeon",
    "platypus",
    "quasar",
    "rhinoceros",
    "river",
    "rustacean",
    "salamander",
    "sitar",
    "stegosaurus",
    "tambourine",
    "tiger",
    "tomato",
    "triceratops",
    "ukulele",
    "viola",
    "weasel",
    "xylophone",
    "yak",
    "zebra",
];

#[cfg(test)]
mod tests {
    use super::*;

    const UUID_1: &str = "a3f7b9c1-e29b-41d4-a716-446655440001";
    const UUID_2: &str = "550e8400-e29b-41d4-a716-446655440002";
    const UUID_3: &str = "6ba7b810-9dad-41d4-80b4-00c04fd430c3";

    fn make_running_entry(id: &str, name: &str, pid: u32) -> SessionEntry {
        SessionEntry {
            id: id.to_string(),
            display_name: name.to_string(),
            pid: Some(pid),
            state: SessionState::Running,
            created_at: "2024-01-15T10:00:00Z".to_string(),
            exited_at: None,
        }
    }

    fn make_exited_entry(id: &str, name: &str) -> SessionEntry {
        SessionEntry {
            id: id.to_string(),
            display_name: name.to_string(),
            pid: None,
            state: SessionState::Exited,
            created_at: "2024-01-14T09:00:00Z".to_string(),
            exited_at: Some("2024-01-14T18:00:00Z".to_string()),
        }
    }

    #[test]
    fn kdl_roundtrip() {
        let registry = SessionRegistry {
            sessions: vec![
                make_running_entry(UUID_1, "my-session", 12345),
                make_exited_entry(UUID_2, "old-session"),
            ],
        };
        let kdl = registry.to_kdl();
        let parsed = SessionRegistry::from_kdl(&kdl).unwrap();
        assert_eq!(parsed.sessions.len(), 2);

        let running = &parsed.sessions[0];
        assert_eq!(running.id, UUID_1);
        assert_eq!(running.display_name, "my-session");
        assert_eq!(running.pid, Some(12345));
        assert_eq!(running.state, SessionState::Running);
        assert_eq!(running.created_at, "2024-01-15T10:00:00Z");
        assert!(running.exited_at.is_none());

        let exited = &parsed.sessions[1];
        assert_eq!(exited.id, UUID_2);
        assert_eq!(exited.display_name, "old-session");
        assert!(exited.pid.is_none());
        assert_eq!(exited.state, SessionState::Exited);
        assert_eq!(exited.exited_at.as_deref(), Some("2024-01-14T18:00:00Z"));
    }

    #[test]
    fn kdl_roundtrip_no_pid_for_exited() {
        let registry = SessionRegistry {
            sessions: vec![make_exited_entry(UUID_1, "dead-session")],
        };
        let kdl = registry.to_kdl();
        assert!(
            !kdl.contains("pid"),
            "exited session should not have pid node"
        );
        let parsed = SessionRegistry::from_kdl(&kdl).unwrap();
        assert!(parsed.sessions[0].pid.is_none());
    }

    #[test]
    fn find_running_by_name_returns_running_not_exited() {
        let registry = SessionRegistry {
            sessions: vec![
                make_running_entry(UUID_1, "foo", 100),
                make_exited_entry(UUID_2, "foo"),
                make_running_entry(UUID_3, "bar", 200),
            ],
        };
        let found = registry.find_running_by_name("foo").unwrap();
        assert_eq!(found.id, UUID_1);
        assert_eq!(found.state, SessionState::Running);
    }

    #[test]
    fn find_running_by_name_returns_none_for_nonexistent() {
        let registry = SessionRegistry {
            sessions: vec![make_running_entry(UUID_1, "foo", 100)],
        };
        assert!(registry.find_running_by_name("nonexistent").is_none());
    }

    #[test]
    fn resolve_socket_path_uses_id() {
        let registry = SessionRegistry {
            sessions: vec![make_running_entry(UUID_1, "my-session", 100)],
        };
        let path = registry.resolve_socket_path("my-session").unwrap();
        assert!(path.ends_with(UUID_1));
    }

    #[test]
    fn remove_by_id() {
        let mut registry = SessionRegistry {
            sessions: vec![
                make_running_entry(UUID_1, "a", 1),
                make_running_entry(UUID_2, "b", 2),
                make_running_entry(UUID_3, "c", 3),
            ],
        };
        registry.remove_by_id(UUID_2);
        assert_eq!(registry.sessions.len(), 2);
        assert!(registry.find_by_id(UUID_2).is_none());
        assert!(registry.find_by_id(UUID_1).is_some());
        assert!(registry.find_by_id(UUID_3).is_some());
    }

    #[test]
    fn generate_session_id_is_valid_uuid() {
        let id = generate_session_id();
        assert_eq!(id.len(), 36);
        assert_eq!(id.as_bytes()[8], b'-');
        assert_eq!(id.as_bytes()[13], b'-');
        assert_eq!(id.as_bytes()[18], b'-');
        assert_eq!(id.as_bytes()[23], b'-');
    }

    #[test]
    fn session_state_roundtrip() {
        assert_eq!(
            SessionState::from_str(SessionState::Running.as_str()),
            Some(SessionState::Running)
        );
        assert_eq!(
            SessionState::from_str(SessionState::Exited.as_str()),
            Some(SessionState::Exited)
        );
        assert_eq!(SessionState::from_str("bogus"), None);
    }
}

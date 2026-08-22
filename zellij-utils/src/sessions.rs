use crate::{
    consts::{
        is_ipc_socket, session_info_folder_for_session, session_layout_cache_file_name,
        ZELLIJ_SESSIONS_KDL, ZELLIJ_SESSIONS_LOCK, ZELLIJ_SESSION_INFO_CACHE_DIR, ZELLIJ_SOCK_DIR,
    },
    data::SessionInfo,
    envs,
    input::layout::Layout,
    ipc::{ClientToServerMsg, IpcReceiverWithContext, IpcSenderWithContext, ServerToClientMsg},
};
use anyhow;
use humantime::format_duration;
use kdl::{KdlDocument, KdlNode, KdlValue};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use std::{fs, io, process};
use suggest::Suggest;
use uuid::Uuid;

// ============================================================================
// Session registry (`sessions.kdl`)
//
// Decouples the user-visible session name from the socket / named-pipe
// filename. The socket/pipe is named by a stable `id` (a short random id for new sessions,
// or the legacy session name for pre-registry sessions); `sessions.kdl` maps
// `id -> display_name`. This is what lets a session be renamed without renaming
// its socket/pipe — impossible for Windows named pipes (see PR #5103).
// ============================================================================

/// A single session entry in the registry.
#[derive(Debug, Clone)]
pub struct SessionEntry {
    /// Stable identifier, also the socket/marker filename (a short random id for
    /// new sessions, or the legacy session name for pre-registry sessions).
    pub id: String,
    /// User-visible session name.
    pub display_name: String,
    /// Server PID (only meaningful while `state == Running`).
    pub pid: Option<u32>,
    /// Running or exited.
    pub state: SessionState,
    /// Unix epoch seconds when the session was created.
    pub created_at: Option<u64>,
    /// Unix epoch seconds when the session exited (only for `Exited`).
    pub exited_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Running,
    Exited,
}

impl SessionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionState::Running => "running",
            SessionState::Exited => "exited",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "running" => Some(SessionState::Running),
            "exited" => Some(SessionState::Exited),
            _ => None,
        }
    }
}

/// Current time as unix epoch seconds.
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A file's creation (or, failing that, modification) time as unix epoch seconds.
fn file_created_epoch(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.created().ok().or_else(|| m.modified().ok()))
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

/// Generate a new session identifier: a short random hex string (48 bits of a
/// UUID v4). Kept short — SESSION_ID_LENGTH bytes — so the socket/pipe path stays
/// within the platform limit (104 bytes on macOS) even under a long temp dir; a
/// full 36-char UUID would overflow it. `register_session` guards against the
/// (astronomically unlikely) collision.
pub fn generate_session_id() -> String {
    let bytes = Uuid::new_v4().into_bytes();
    let mut id = String::with_capacity(crate::consts::SESSION_ID_LENGTH);
    for byte in &bytes[..crate::consts::SESSION_ID_LENGTH / 2] {
        id.push_str(&format!("{:02x}", byte));
    }
    id
}

fn is_registry_file(file_name: &str) -> bool {
    // Covers sessions.kdl, sessions.kdl.lock, sessions.kdl.bak, and the
    // sessions.kdl.tmp.<pid> scratch files written by atomic_write. On Windows
    // these are regular files that would otherwise look like socket markers.
    file_name.starts_with("sessions.kdl")
}

/// The `<path>.bak` sibling used to keep the last good registry.
fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

/// Atomically replace `path` with `contents`: write a sibling temp file, flush
/// it, back up the current file to `<path>.bak`, then rename over the target
/// (atomic on the same filesystem). A crash mid-write leaves the old file (or
/// its backup) intact rather than a truncated one.
fn atomic_write(path: &Path, contents: &str) -> io::Result<()> {
    use std::io::Write;
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(format!(".tmp.{}", process::id()));
    let tmp = path.with_file_name(tmp_name);
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(contents.as_bytes())?;
        f.flush()?;
        let _ = f.sync_all();
    }
    if path.exists() {
        // Best-effort backup of the last good file before we replace it.
        let _ = fs::copy(path, backup_path(path));
    }
    fs::rename(&tmp, path)
}

/// Load a registry for a read-modify-write. Unlike the read-only path, a
/// corrupt (non-empty, unparseable) *existing* file is an error here: we must
/// not overwrite real session data with an empty registry.
fn load_registry_for_write(read: io::Result<String>) -> io::Result<SessionRegistry> {
    match read {
        Ok(raw) if raw.trim().is_empty() => Ok(SessionRegistry::new()),
        Ok(raw) => SessionRegistry::from_kdl(&raw)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
        Err(ref e) if e.kind() == io::ErrorKind::NotFound => Ok(SessionRegistry::new()),
        Err(e) => Err(e),
    }
}

/// The full session registry.
#[derive(Debug, Clone, Default)]
pub struct SessionRegistry {
    pub sessions: Vec<SessionEntry>,
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
            let child_string = |key: &str| -> Option<String> {
                children
                    .get(key)
                    .and_then(|n| n.entries().iter().next())
                    .and_then(|e| e.value().as_string())
                    .map(|s| s.to_string())
            };
            let child_i64 = |key: &str| -> Option<i64> {
                children
                    .get(key)
                    .and_then(|n| n.entries().iter().next())
                    .and_then(|e| e.value().as_i64())
            };
            let display_name = child_string("display_name").unwrap_or_default();
            let pid = child_i64("pid").map(|v| v as u32);
            let state = child_string("state")
                .as_deref()
                .and_then(SessionState::parse)
                .unwrap_or(SessionState::Running);
            let created_at = child_i64("created_at").map(|v| v as u64);
            let exited_at = child_i64("exited_at").map(|v| v as u64);
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
            let mut push_child = |name: &str, value: KdlValue| {
                let mut n = KdlNode::new(name);
                n.push(value);
                children.nodes_mut().push(n);
            };

            push_child("display_name", KdlValue::String(entry.display_name.clone()));
            if let Some(pid) = entry.pid {
                push_child("pid", KdlValue::Base10(pid as i64));
            }
            push_child("state", KdlValue::String(entry.state.as_str().to_string()));
            if let Some(created_at) = entry.created_at {
                push_child("created_at", KdlValue::Base10(created_at as i64));
            }
            if let Some(exited_at) = entry.exited_at {
                push_child("exited_at", KdlValue::Base10(exited_at as i64));
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

    /// Find a session by id.
    pub fn find_by_id(&self, id: &str) -> Option<&SessionEntry> {
        self.sessions.iter().find(|s| s.id == id)
    }

    /// Find a mutable session by id.
    pub fn find_by_id_mut(&mut self, id: &str) -> Option<&mut SessionEntry> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    /// Iterate over running sessions.
    pub fn running_sessions(&self) -> impl Iterator<Item = &SessionEntry> {
        self.sessions
            .iter()
            .filter(|s| s.state == SessionState::Running)
    }

    /// Remove a session by id.
    pub fn remove_by_id(&mut self, id: &str) {
        self.sessions.retain(|s| s.id != id);
    }

    /// Resolve a running session display name to its socket path.
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

    /// An advisory exclusive lock held for the lifetime of the value. The lock
    /// is released when the file is dropped (fd closed → `flock` released).
    pub struct FileLock {
        _file: File,
    }

    impl FileLock {
        pub fn exclusive(path: &Path) -> std::io::Result<Self> {
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(path)?;
            let ret = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
            if ret != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(FileLock { _file: file })
        }
    }
}

#[cfg(windows)]
mod file_lock {
    use std::fs::{File, OpenOptions};
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;

    /// An exclusive lock held for the lifetime of the value. The lock is
    /// released when the file is dropped (handle closed → lock released).
    pub struct FileLock {
        _file: File,
    }

    impl FileLock {
        pub fn exclusive(path: &Path) -> std::io::Result<Self> {
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(path)?;
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
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(path)?;
            Ok(FileLock { _file: file })
        }
    }
}

use file_lock::FileLock;

/// Ensure the socket dir exists so the lock/registry files can live in it.
fn ensure_sock_dir() -> io::Result<()> {
    fs::create_dir_all(&*ZELLIJ_SOCK_DIR)
}

/// Returns true if the session registry file exists on disk.
pub fn registry_exists() -> bool {
    ZELLIJ_SESSIONS_KDL.exists()
}

/// Read the registry from disk, returning an empty one if it's missing. A
/// corrupt file falls back to the `.bak` backup before giving up empty.
pub fn read_registry() -> SessionRegistry {
    match fs::read_to_string(&*ZELLIJ_SESSIONS_KDL) {
        Ok(raw) => SessionRegistry::from_kdl(&raw).unwrap_or_else(|e| {
            log::error!(
                "Corrupt {}: {}; falling back to backup",
                ZELLIJ_SESSIONS_KDL.display(),
                e
            );
            read_registry_backup().unwrap_or_default()
        }),
        Err(_) => SessionRegistry::new(),
    }
}

/// Try to read the last good registry from the `.bak` backup file.
fn read_registry_backup() -> Option<SessionRegistry> {
    let raw = fs::read_to_string(backup_path(&ZELLIJ_SESSIONS_KDL)).ok()?;
    SessionRegistry::from_kdl(&raw).ok()
}

/// Ensure the registry exists, migrating from the legacy socket-named layout on
/// first use. Returns the current registry.
pub fn ensure_registry() -> SessionRegistry {
    if registry_exists() {
        read_registry()
    } else {
        migrate_legacy_sessions()
    }
}

/// Migrate legacy (pre-registry) sessions into a new `sessions.kdl`.
///
/// Scans `ZELLIJ_SOCK_DIR` for old socket/marker files (named by session name)
/// and the session-info cache for resurrectable sessions, creating registry
/// entries with `id == existing_name` so lookups keep working without moving
/// any files. Called once when `sessions.kdl` doesn't exist yet.
pub fn migrate_legacy_sessions() -> SessionRegistry {
    let mut registry = SessionRegistry::new();

    // Live legacy sessions: socket/marker files named directly by session name.
    if let Ok(files) = fs::read_dir(&*ZELLIJ_SOCK_DIR) {
        for file in files.flatten() {
            let file_name = match file.file_name().into_string() {
                Ok(n) => n,
                Err(_) => continue,
            };
            if is_registry_file(&file_name) {
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
            // The filename IS the legacy session name; keep it as the id so
            // ipc_connect still finds it by joining ZELLIJ_SOCK_DIR with the id.
            #[cfg(windows)]
            let pid = fs::read_to_string(file.path())
                .ok()
                .and_then(|s| s.lines().next().and_then(|l| l.trim().parse::<u32>().ok()));
            #[cfg(not(windows))]
            let pid: Option<u32> = None;

            registry.sessions.push(SessionEntry {
                id: file_name.clone(),
                display_name: file_name,
                pid,
                state: SessionState::Running,
                created_at: file_created_epoch(&file.path()),
                exited_at: None,
            });
        }
    }

    // Resurrectable legacy sessions from the (name-keyed) session-info cache.
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
            if registry.find_by_name(&session_name).is_some() {
                continue;
            }
            let layout_file = session_layout_cache_file_name(&session_name);
            if !Path::new(&layout_file).exists() {
                continue;
            }
            let created_at = file_created_epoch(&layout_file);
            // Legacy resurrectable entry: id == name so the (name-keyed) cache
            // folder is still found by id.
            registry.sessions.push(SessionEntry {
                id: session_name.clone(),
                display_name: session_name,
                pid: None,
                state: SessionState::Exited,
                created_at,
                exited_at: created_at,
            });
        }
    }

    if let Err(e) = write_registry(&registry) {
        log::error!("Failed to write migrated session registry: {:?}", e);
    }
    registry
}

/// Write the session registry to disk atomically, holding an exclusive lock.
pub fn write_registry(registry: &SessionRegistry) -> io::Result<()> {
    ensure_sock_dir()?;
    let _lock = FileLock::exclusive(&ZELLIJ_SESSIONS_LOCK)?;
    atomic_write(&ZELLIJ_SESSIONS_KDL, &registry.to_kdl())
}

/// Read the registry under an exclusive lock, apply a mutation, and write it
/// back atomically. Returns the closure's return value.
///
/// If the existing file is present but corrupt, the mutation is aborted (an
/// error is returned) rather than overwriting real session data with an empty
/// registry.
///
/// The closure MUST NOT call back into the registry (`with_registry`,
/// `write_registry`, `ensure_registry`) — the lock is not reentrant.
pub fn with_registry<F, R>(f: F) -> io::Result<R>
where
    F: FnOnce(&mut SessionRegistry) -> R,
{
    ensure_sock_dir()?;
    let _lock = FileLock::exclusive(&ZELLIJ_SESSIONS_LOCK)?;
    let mut registry =
        load_registry_for_write(fs::read_to_string(&*ZELLIJ_SESSIONS_KDL)).map_err(|e| {
            log::error!(
                "Refusing to modify corrupt {}: {}",
                ZELLIJ_SESSIONS_KDL.display(),
                e
            );
            e
        })?;
    let result = f(&mut registry);
    atomic_write(&ZELLIJ_SESSIONS_KDL, &registry.to_kdl())?;
    Ok(result)
}

/// Register a new session in the registry. Returns the generated id.
pub fn register_session(display_name: &str) -> io::Result<String> {
    with_registry(|reg| {
        // Resurrecting a dead session reuses its id (and therefore its id-keyed
        // cache folder / layout), rather than orphaning the old folder under a
        // fresh id.
        if let Some(existing) = reg
            .sessions
            .iter_mut()
            .find(|s| s.display_name == display_name && s.state == SessionState::Exited)
        {
            existing.state = SessionState::Running;
            existing.pid = None;
            existing.exited_at = None;
            existing.created_at = Some(now_secs());
            return existing.id.clone();
        }
        // Generate a fresh id, guarding against the unlikely collision with an
        // existing entry (the id space is short — see generate_session_id).
        let id = loop {
            let candidate = generate_session_id();
            if !reg.sessions.iter().any(|s| s.id == candidate) {
                break candidate;
            }
        };
        reg.sessions.push(SessionEntry {
            id: id.clone(),
            display_name: display_name.to_string(),
            pid: None,
            state: SessionState::Running,
            created_at: Some(now_secs()),
            exited_at: None,
        });
        id
    })
}

/// Resolve a session display name to its socket path via the registry.
pub fn resolve_session_socket_path(name: &str) -> Option<PathBuf> {
    ensure_registry().resolve_socket_path(name)
}

/// Resolve a session display name to its id via the registry (running first,
/// then any state). Returns None if the name isn't in the registry — callers
/// fall back to treating the name itself as the id (legacy sessions).
pub fn resolve_session_id(name: &str) -> Option<String> {
    let registry = ensure_registry();
    registry
        .find_running_by_name(name)
        .or_else(|| registry.find_by_name(name))
        .map(|e| e.id.clone())
}

/// The display name for an id, or the id itself if it isn't in the registry
/// (legacy / orphaned cache folders whose id is the old session name).
fn display_name_for_id(registry: &SessionRegistry, id: &str) -> String {
    registry
        .find_by_id(id)
        .map(|e| e.display_name.clone())
        .unwrap_or_else(|| id.to_string())
}

/// Scan the session-info cache for folders that hold a resurrectable layout,
/// returning (folder_id, age). The cache is keyed by session id.
fn resurrectable_folder_entries() -> Vec<(String, Duration)> {
    let Ok(dirs) = fs::read_dir(&*ZELLIJ_SESSION_INFO_CACHE_DIR) else {
        return vec![];
    };
    dirs.filter_map(|f| f.ok().map(|f| f.path()))
        .filter(|p| p.is_dir())
        .filter_map(|folder| {
            let id = folder.file_name()?.to_str()?.to_owned();
            let layout_file = folder.join("session-layout.kdl");
            if !layout_file.exists() {
                return None;
            }
            // Try creation time, fall back to modification time (e.g. musl).
            let elapsed = std::fs::metadata(&layout_file)
                .ok()
                .and_then(|m| m.created().ok().or_else(|| m.modified().ok()))
                .and_then(|t| t.elapsed().ok())
                .map(|d| Duration::from_secs(d.as_secs()))
                .unwrap_or_default();
            Some((id, elapsed))
        })
        .collect()
}

/// What to do with a registry entry during reconciliation, given the current
/// filesystem reality.
#[derive(Debug, PartialEq, Eq)]
enum ReconcileAction {
    /// Keep the entry unchanged.
    Keep,
    /// The server is gone but a resurrection layout exists — mark it exited.
    MarkExited,
    /// The server is gone and nothing is resurrectable — drop the entry.
    Drop,
}

fn reconcile_decision(
    state: SessionState,
    socket_alive: bool,
    layout_exists: bool,
) -> ReconcileAction {
    match state {
        SessionState::Running if socket_alive => ReconcileAction::Keep,
        SessionState::Running if layout_exists => ReconcileAction::MarkExited,
        SessionState::Running => ReconcileAction::Drop,
        SessionState::Exited if layout_exists => ReconcileAction::Keep,
        SessionState::Exited => ReconcileAction::Drop,
    }
}

/// Reconcile the registry with the filesystem, bounding its growth: running
/// entries whose socket is gone become exited (if a resurrection layout exists)
/// or are dropped; exited entries whose layout is gone are dropped. Probing also
/// cleans up stale socket files (via `assert_socket`). The file is only
/// rewritten when something actually changed. Returns the reconciled registry.
pub fn reconcile_registry() -> SessionRegistry {
    // Make sure legacy sessions are migrated in before reconciling.
    ensure_registry();
    if let Err(e) = reconcile_locked() {
        log::error!("Failed to reconcile session registry: {:?}", e);
    }
    read_registry()
}

fn reconcile_locked() -> io::Result<()> {
    ensure_sock_dir()?;
    let _lock = FileLock::exclusive(&ZELLIJ_SESSIONS_LOCK)?;
    let mut registry = load_registry_for_write(fs::read_to_string(&*ZELLIJ_SESSIONS_KDL))?;
    let mut changed = false;
    registry.sessions.retain_mut(|e| {
        let socket_alive = e.state == SessionState::Running && assert_socket(&e.id);
        let layout_exists = session_layout_cache_file_name(&e.id).exists();
        match reconcile_decision(e.state, socket_alive, layout_exists) {
            ReconcileAction::Keep => true,
            ReconcileAction::MarkExited => {
                e.state = SessionState::Exited;
                e.pid = None;
                if e.exited_at.is_none() {
                    e.exited_at = Some(now_secs());
                }
                changed = true;
                true
            },
            ReconcileAction::Drop => {
                changed = true;
                false
            },
        }
    });
    if changed {
        atomic_write(&ZELLIJ_SESSIONS_KDL, &registry.to_kdl())?;
    }
    Ok(())
}

pub fn get_sessions() -> Result<Vec<(String, Duration)>, io::ErrorKind> {
    // Reconcile first: prunes dead entries and probes liveness, so the remaining
    // running entries are known to be alive.
    let registry = reconcile_registry();
    let mut sessions = Vec::new();
    for entry in registry.running_sessions() {
        let sock_path = ZELLIJ_SOCK_DIR.join(&entry.id);
        let ctime = std::fs::metadata(&sock_path)
            .ok()
            .and_then(|f| f.created().ok().or_else(|| f.modified().ok()))
            .and_then(|d| d.elapsed().ok())
            .unwrap_or_default();
        sessions.push((
            entry.display_name.clone(),
            Duration::from_secs(ctime.as_secs()),
        ));
    }
    Ok(sessions)
}

pub fn get_resurrectable_sessions() -> Vec<(String, Duration)> {
    let registry = ensure_registry();
    resurrectable_folder_entries()
        .into_iter()
        .map(|(id, elapsed)| (display_name_for_id(&registry, &id), elapsed))
        .collect()
}

pub fn get_resurrectable_session_names() -> Vec<String> {
    let registry = ensure_registry();
    resurrectable_folder_entries()
        .into_iter()
        .map(|(id, _)| display_name_for_id(&registry, &id))
        .collect()
}

pub fn get_sessions_sorted_by_mtime() -> anyhow::Result<Vec<String>> {
    let registry = reconcile_registry();
    let mut sessions_with_mtime: Vec<(String, SystemTime)> = Vec::new();
    for entry in registry.running_sessions() {
        let sock_path = ZELLIJ_SOCK_DIR.join(&entry.id);
        if let Ok(mtime) = std::fs::metadata(&sock_path).and_then(|m| m.modified()) {
            sessions_with_mtime.push((entry.display_name.clone(), mtime));
        }
    }
    sessions_with_mtime.sort_by_key(|x| x.1); // the oldest one will be the first
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
        if handle.is_null() {
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
    // The cache folder is keyed by session id; resolve the name (fall back to
    // treating the name as the id for legacy sessions).
    let id = resolve_session_id(name).unwrap_or_else(|| name.to_string());
    // Drop the registry entry (running or exited) so the id is not left dangling.
    let _ = with_registry(|reg| {
        reg.sessions
            .retain(|s| s.id != id && s.display_name != name)
    });
    if let Err(e) = std::fs::remove_dir_all(session_info_folder_for_session(&id)) {
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
    // The layout cache is keyed by session id; resolve the name (fall back to
    // treating the name as the id for legacy sessions).
    let id = resolve_session_id(session_name_to_resurrect)
        .unwrap_or_else(|| session_name_to_resurrect.to_string());
    let layout_file_name = session_layout_cache_file_name(&id);
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

pub fn session_listing_error_message(kind: io::ErrorKind) -> String {
    format!(
        "Failed to list existing Zellij sessions in the socket directory:\n  {}\n\n\
         Reason: {}\n\n\
         This usually means the directory (or one of its parents) is not readable \
         by the current user - for example if $ZELLIJ_SOCKET_DIR or $XDG_RUNTIME_DIR \
         points to a directory you do not own.\n\
         To fix this, set a readable and writable socket directory, eg.:\n  \
         ZELLIJ_SOCKET_DIR=/tmp/zellij-$USER zellij",
        ZELLIJ_SOCK_DIR.display(),
        io::Error::from(kind)
    )
}

pub fn read_live_session_states(
    current_session_name: &str,
    sock_dir: &Path,
    session_info_cache_dir: &Path,
) -> BTreeMap<String, SessionInfo> {
    let mut session_infos_on_machine = BTreeMap::new();
    // The registry maps id-named sockets to their user-visible display names.
    // For legacy or test sockets named directly after the session, the lookup
    // misses and we fall back to the filename.
    let registry = ensure_registry();
    let files = match fs::read_dir(sock_dir) {
        Ok(files) => files,
        Err(_) => return session_infos_on_machine,
    };
    for file in files.flatten() {
        let file_name = match file.file_name().into_string() {
            Ok(n) => n,
            Err(_) => continue,
        };
        if is_registry_file(&file_name) {
            continue;
        }
        let is_socket = file
            .file_type()
            .map(|file_type| is_ipc_socket(&file_type))
            .unwrap_or(false);
        if !is_socket {
            continue;
        }
        let session_name = registry
            .find_by_id(&file_name)
            .map(|e| e.display_name.clone())
            .unwrap_or_else(|| file_name.clone());
        let creation_time = std::fs::metadata(file.path())
            .ok()
            .and_then(|f| f.created().ok().or_else(|| f.modified().ok()))
            .and_then(|d| d.elapsed().ok())
            .unwrap_or_default();
        // The session-info cache is keyed by session id (the socket file name).
        let session_cache_file_name = session_info_cache_dir
            .join(&file_name)
            .join("session-metadata.kdl");
        if let Ok(raw_session_info) = fs::read_to_string(&session_cache_file_name) {
            if let Ok(mut session_info) =
                SessionInfo::from_string(&raw_session_info, current_session_name)
            {
                session_info.creation_time = creation_time;
                session_infos_on_machine.insert(session_name, session_info);
            }
        }
    }
    session_infos_on_machine
}

pub fn read_live_session_states_default_dirs(
    current_session_name: &str,
) -> BTreeMap<String, SessionInfo> {
    read_live_session_states(
        current_session_name,
        &*ZELLIJ_SOCK_DIR,
        &*ZELLIJ_SESSION_INFO_CACHE_DIR,
    )
}

pub fn generate_unique_session_name() -> Option<String> {
    let sessions = get_sessions().map(|sessions| {
        sessions
            .iter()
            .map(|s| s.0.clone())
            .collect::<Vec<String>>()
    });
    let dead_sessions = get_resurrectable_session_names();
    let sessions = match sessions {
        Ok(sessions) => sessions,
        Err(kind) => {
            eprintln!("{}", session_listing_error_message(kind));
            return None;
        },
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
    "iguanodon",
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
mod registry_tests {
    use super::*;

    const UUID_1: &str = "a3f7b9c1-e29b-41d4-a716-446655440001";
    const UUID_2: &str = "550e8400-e29b-41d4-a716-446655440002";
    const UUID_3: &str = "6ba7b810-9dad-41d4-80b4-00c04fd430c3";

    fn running(id: &str, name: &str, pid: u32) -> SessionEntry {
        SessionEntry {
            id: id.to_string(),
            display_name: name.to_string(),
            pid: Some(pid),
            state: SessionState::Running,
            created_at: Some(1_700_000_000),
            exited_at: None,
        }
    }

    fn exited(id: &str, name: &str) -> SessionEntry {
        SessionEntry {
            id: id.to_string(),
            display_name: name.to_string(),
            pid: None,
            state: SessionState::Exited,
            created_at: Some(1_700_000_000),
            exited_at: Some(1_700_000_500),
        }
    }

    #[test]
    fn kdl_roundtrip() {
        let registry = SessionRegistry {
            sessions: vec![
                running(UUID_1, "my-session", 12345),
                exited(UUID_2, "old-session"),
            ],
        };
        let parsed = SessionRegistry::from_kdl(&registry.to_kdl()).unwrap();
        assert_eq!(parsed.sessions.len(), 2);

        let r = parsed.find_by_id(UUID_1).unwrap();
        assert_eq!(r.display_name, "my-session");
        assert_eq!(r.pid, Some(12345));
        assert_eq!(r.state, SessionState::Running);
        assert_eq!(r.created_at, Some(1_700_000_000));
        assert!(r.exited_at.is_none());

        let e = parsed.find_by_id(UUID_2).unwrap();
        assert_eq!(e.state, SessionState::Exited);
        assert!(e.pid.is_none());
        assert_eq!(e.exited_at, Some(1_700_000_500));
    }

    #[test]
    fn kdl_omits_pid_for_exited() {
        let registry = SessionRegistry {
            sessions: vec![exited(UUID_1, "dead")],
        };
        let kdl = registry.to_kdl();
        assert!(
            !kdl.contains("pid"),
            "exited session should have no pid node: {}",
            kdl
        );
        assert!(SessionRegistry::from_kdl(&kdl).unwrap().sessions[0]
            .pid
            .is_none());
    }

    #[test]
    fn find_running_by_name_prefers_running() {
        let registry = SessionRegistry {
            sessions: vec![
                exited(UUID_1, "foo"),
                running(UUID_2, "foo", 100),
                running(UUID_3, "bar", 200),
            ],
        };
        let found = registry.find_running_by_name("foo").unwrap();
        assert_eq!(found.id, UUID_2);
        assert_eq!(found.state, SessionState::Running);
        assert!(registry.find_running_by_name("missing").is_none());
    }

    #[test]
    fn resolve_socket_path_uses_id() {
        let registry = SessionRegistry {
            sessions: vec![running(UUID_1, "my-session", 100)],
        };
        let path = registry.resolve_socket_path("my-session").unwrap();
        assert!(path.ends_with(UUID_1));
        assert!(registry.resolve_socket_path("nope").is_none());
    }

    #[test]
    fn remove_by_id() {
        let mut registry = SessionRegistry {
            sessions: vec![
                running(UUID_1, "a", 1),
                running(UUID_2, "b", 2),
                running(UUID_3, "c", 3),
            ],
        };
        registry.remove_by_id(UUID_2);
        assert_eq!(registry.sessions.len(), 2);
        assert!(registry.find_by_id(UUID_2).is_none());
        assert!(registry.find_by_id(UUID_1).is_some());
    }

    #[test]
    fn running_sessions_filters_state() {
        let registry = SessionRegistry {
            sessions: vec![
                running(UUID_1, "a", 1),
                exited(UUID_2, "b"),
                running(UUID_3, "c", 3),
            ],
        };
        let names: Vec<_> = registry
            .running_sessions()
            .map(|e| e.display_name.clone())
            .collect();
        assert_eq!(names, vec!["a", "c"]);
    }

    #[test]
    fn generated_id_is_short_hex() {
        let id = generate_session_id();
        assert_eq!(id.len(), crate::consts::SESSION_ID_LENGTH);
        assert!(
            id.bytes().all(|b| b.is_ascii_hexdigit()),
            "id should be hex: {}",
            id
        );
        // Two calls should differ (random).
        assert_ne!(id, generate_session_id());
    }

    #[test]
    fn session_state_str_roundtrip() {
        assert_eq!(
            SessionState::parse(SessionState::Running.as_str()),
            Some(SessionState::Running)
        );
        assert_eq!(
            SessionState::parse(SessionState::Exited.as_str()),
            Some(SessionState::Exited)
        );
        assert_eq!(SessionState::parse("bogus"), None);
    }

    #[test]
    fn unknown_state_defaults_to_running() {
        let kdl = "session \"id-x\" {\n    display_name \"x\"\n    state \"weird\"\n}";
        let parsed = SessionRegistry::from_kdl(kdl).unwrap();
        assert_eq!(parsed.sessions[0].state, SessionState::Running);
    }

    #[test]
    fn atomic_write_replaces_and_backs_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sessions.kdl");

        atomic_write(&path, "first").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "first");
        assert!(!backup_path(&path).exists(), "no backup on first write");

        atomic_write(&path, "second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), "first");

        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp."))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {:?}",
            leftovers
        );
    }

    #[test]
    fn load_for_write_rejects_corrupt_but_accepts_empty_and_missing() {
        // Corrupt, non-empty → error, so we never overwrite real data with empty.
        let err = load_registry_for_write(Ok("node {".to_string())).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);

        // Empty / whitespace-only → fresh empty registry.
        assert!(load_registry_for_write(Ok("   \n".to_string()))
            .unwrap()
            .sessions
            .is_empty());

        // Missing file → fresh empty registry.
        let notfound = io::Error::new(io::ErrorKind::NotFound, "nope");
        assert!(load_registry_for_write(Err(notfound))
            .unwrap()
            .sessions
            .is_empty());

        // Any other IO error → propagated (don't silently start empty).
        let denied = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        assert!(load_registry_for_write(Err(denied)).is_err());
    }

    #[test]
    fn reconcile_decisions() {
        use ReconcileAction::*;
        use SessionState::*;
        // Running + live socket → keep, regardless of layout.
        assert_eq!(reconcile_decision(Running, true, false), Keep);
        assert_eq!(reconcile_decision(Running, true, true), Keep);
        // Running + dead socket → exit if resurrectable, else drop.
        assert_eq!(reconcile_decision(Running, false, true), MarkExited);
        assert_eq!(reconcile_decision(Running, false, false), Drop);
        // Exited → keep only while the resurrection layout exists.
        assert_eq!(reconcile_decision(Exited, false, true), Keep);
        assert_eq!(reconcile_decision(Exited, false, false), Drop);
    }
}

#[cfg(any(windows, test))]
use crate::ipc::IpcReceiverWithContext;
use crate::{
    consts::{
        is_ipc_socket, session_deletion_tombstone_path, session_incarnation_path,
        session_layout_cache_file_name, session_publication_lock_path,
        ZELLIJ_SESSION_INFO_CACHE_DIR, ZELLIJ_SOCK_DIR,
    },
    data::SessionInfo,
    envs,
    input::layout::Layout,
    ipc::{ClientToServerMsg, IpcSenderWithContext, ServerToClientMsg},
    session_index::{self, SessionIndex},
};
use anyhow;
use humantime::format_duration;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};
use std::{fs, io, process};
use suggest::Suggest;
use uuid::Uuid;

// A session's socket/named pipe and its session-info cache folder are named by a short, stable
// session id rather than by the session's name, so renaming a session renames nothing on disk
// (Windows named pipes cannot be renamed). The session index maps names to ids. Sessions it
// doesn't know (started by versions from before ids) use their name as their id.

/// Generate a new session id: a short random hex string (48 bits of a UUID v4). It is kept to
/// SESSION_ID_LENGTH bytes so that the socket path stays within the platform limit (104 bytes on
/// macOS) even under a long temp dir, which a full 36-char UUID would overflow.
/// `SessionIndex::register` guards against the (astronomically unlikely) collision.
pub fn generate_session_id() -> String {
    let bytes = Uuid::new_v4().into_bytes();
    let mut id = String::with_capacity(crate::consts::SESSION_ID_LENGTH);
    for byte in &bytes[..crate::consts::SESSION_ID_LENGTH / 2] {
        id.push_str(&format!("{:02x}", byte));
    }
    id
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Register a session that is about to start and return the id to name its socket by. A session
/// being resurrected keeps its id, and so its cache folder. Without a usable session index the
/// session is named by its name, as before ids existed.
pub fn register_session(name: &str) -> String {
    match SessionIndex::open_default().and_then(|index| index.register(name, now_secs())) {
        Ok(id) => id,
        Err(e) => {
            log::error!(
                "Failed to register session {:?} in the session index, naming its socket after it: {}",
                name,
                e
            );
            name.to_owned()
        },
    }
}

/// Record a session's new name. Its socket and cache folder keep their id-based names.
pub fn rename_session(session_id: &str, new_name: &str) -> Result<(), String> {
    SessionIndex::open_default()
        .and_then(|index| index.rename_by_id(session_id, new_name, now_secs()))
        .map_err(|e| {
            format!(
                "Failed to record the new name of session {:?} in the session index: {}",
                session_id, e
            )
        })
        .and_then(|outcome| match outcome {
            session_index::RenameOutcome::Renamed => Ok(()),
            session_index::RenameOutcome::Conflict => {
                Err("A session by this name already exists.".to_owned())
            },
        })
}

/// The id of the session with this name.
///
/// `Ok(None)` means the index is missing or has no row: a session from before ids, whose
/// folder is the name. `Err` is a failed read and must not be treated as that.
pub fn resolve_session_id(name: &str) -> Result<Option<String>, String> {
    session_index::session_id_for_name(name).map_err(|error| error.to_string())
}

/// The socket path of the session with this name, if the session index knows it.
///
/// `Ok(None)` means the index does not know this name. A database error is `Err`, not `Ok(None)`.
pub fn resolve_session_socket_path(name: &str) -> Result<Option<PathBuf>, String> {
    Ok(resolve_session_id(name)?.map(|id| ZELLIJ_SOCK_DIR.join(id)))
}

/// Folder id for `name`: the index id, or `name` when the index has no row.
/// A read error is `Err` and is not `name`.
pub fn session_folder_id(name: &str) -> Result<String, String> {
    session_index::folder_id_from_lookup(name, session_index::session_id_for_name(name))
        .map_err(|error| error.to_string())
}

/// The name of the session with this id, or the id when the index has no row.
/// A lookup error is logged and also shown as the id. Deleting an index row must use
/// [`session_index::session_name_for_id`] and stop on `Err` instead of this fallback.
pub fn display_name_for_session_id(id: &str) -> String {
    match session_index::session_name_for_id(id) {
        Ok(Some(name)) => name,
        Ok(None) => id.to_owned(),
        Err(error) => {
            log::error!("Failed to look up the name of session {:?}: {}", id, error);
            id.to_owned()
        },
    }
}

fn session_name(names_by_id: &HashMap<String, String>, id: String) -> String {
    names_by_id.get(&id).cloned().unwrap_or(id)
}

/// How long the socket of a single session may take to answer before that
/// session is treated as unknown (not dead), and how long listing may take in total.
const SESSION_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
/// Upper bound on sockets probed at once, so a directory full of wedged
/// sockets is scanned in parallel without one thread per entry.
const MAX_CONCURRENT_SESSION_PROBES: usize = 8;

/// Result of probing one session socket / marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionProbeResult {
    /// The server answered and is confirmed live.
    Responsive,
    /// A socket or marker exists but liveness could not be established (timeout,
    /// unexpected reply, access denied, unreadable marker, or never probed before
    /// the shared deadline). Must not be treated as absent.
    Unknown,
    /// Proven gone: no socket, connection refused (and cleaned), or process death
    /// established by platform-specific evidence.
    Absent,
}

/// Bounded discovery outcome: responsive sessions plus candidates that stayed unknown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionObservation {
    pub responsive: Vec<(String, Duration)>,
    pub unknown: Vec<(String, Duration)>,
}

impl SessionObservation {
    pub fn is_definitely_empty(&self) -> bool {
        self.responsive.is_empty() && self.unknown.is_empty()
    }
}

/// The socket/named-pipe file name of a directory entry, if it could be a session.
fn session_socket_id(file: &fs::DirEntry) -> Option<String> {
    let file_name = file.file_name().into_string().ok()?;
    let file_type = file.file_type().ok()?;
    if crate::consts::is_app_socket_name(&file_name) || !is_ipc_socket(&file_type) {
        return None;
    }
    Some(file_name)
}

fn session_age(file: &fs::DirEntry) -> Duration {
    // Try to get creation time, falling back to modification time on platforms where it is not
    // supported (for example musl). These are almost always identical for sessions; name changes
    // are the notable exception.
    let age = fs::metadata(file.path())
        .ok()
        .and_then(|metadata| metadata.created().ok().or_else(|| metadata.modified().ok()))
        .and_then(|time| time.elapsed().ok())
        .unwrap_or_default();
    Duration::from_secs(age.as_secs())
}

/// Probe `(id, name, payload)` candidates concurrently and return the responsive
/// ones in candidate order. Unknown / unprobed candidates are omitted here; use
/// [`observe_sessions_with`] when the caller must distinguish unknown from absent.
fn live_sessions<T: Clone + Send + Sync>(
    dir: &Path,
    candidates: Vec<(String, String, T)>,
    deadline: Duration,
) -> Vec<(String, T)> {
    observe_sessions_with(candidates, deadline, |id, remaining| {
        assert_socket_in(dir, id, remaining)
    })
    .responsive
}

#[derive(Clone, Copy)]
enum ObservedSlot {
    Responsive,
    Unknown,
}

fn observe_sessions_with<T, F>(
    candidates: Vec<(String, String, T)>,
    deadline: Duration,
    probe: F,
) -> SessionObservationPayload<T>
where
    T: Clone + Send + Sync,
    F: Fn(&str, Duration) -> SessionProbeResult + Sync,
{
    let mut observation = SessionObservationPayload {
        responsive: Vec::new(),
        unknown: Vec::new(),
    };
    if candidates.is_empty() {
        return observation;
    }
    if candidates.len() == 1 {
        let (id, name, payload) = candidates.into_iter().next().unwrap();
        match probe(&id, deadline) {
            SessionProbeResult::Responsive => observation.responsive.push((name, payload)),
            SessionProbeResult::Unknown => observation.unknown.push((name, payload)),
            SessionProbeResult::Absent => {},
        }
        return observation;
    }
    let results: Vec<Mutex<Option<(ObservedSlot, String, T)>>> =
        candidates.iter().map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let started = Instant::now();
    let workers = candidates.len().min(MAX_CONCURRENT_SESSION_PROBES);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some((id, name, _)) = candidates.get(index) else {
                    break;
                };
                let remaining = deadline.saturating_sub(started.elapsed());
                let outcome = if remaining.is_zero() {
                    // Never reached before the shared deadline: unknown, not absent.
                    SessionProbeResult::Unknown
                } else {
                    probe(id, remaining)
                };
                let slot = match outcome {
                    SessionProbeResult::Responsive => Some(ObservedSlot::Responsive),
                    SessionProbeResult::Unknown => Some(ObservedSlot::Unknown),
                    SessionProbeResult::Absent => None,
                };
                if let Some(slot) = slot {
                    let payload = candidates[index].2.clone();
                    *results[index]
                        .lock()
                        .expect("session probe result lock poisoned") =
                        Some((slot, name.clone(), payload));
                }
            });
        }
    });
    for slot in results {
        if let Some((kind, name, payload)) = slot.into_inner().unwrap_or_else(|e| e.into_inner()) {
            match kind {
                ObservedSlot::Responsive => observation.responsive.push((name, payload)),
                ObservedSlot::Unknown => observation.unknown.push((name, payload)),
            }
        }
    }
    observation
}

struct SessionObservationPayload<T> {
    responsive: Vec<(String, T)>,
    unknown: Vec<(String, T)>,
}

pub fn get_sessions() -> Result<Vec<(String, Duration)>, io::ErrorKind> {
    Ok(observe_sessions()?.responsive)
}

/// Discover sessions while preserving unknown/unresponsive candidates separately from absent.
pub fn observe_sessions() -> Result<SessionObservation, io::ErrorKind> {
    observe_sessions_in(&ZELLIJ_SOCK_DIR)
}

fn observe_sessions_in(dir: &Path) -> Result<SessionObservation, io::ErrorKind> {
    observe_sessions_in_with(dir, SESSION_PROBE_TIMEOUT)
}

fn observe_sessions_in_with(
    dir: &Path,
    deadline: Duration,
) -> Result<SessionObservation, io::ErrorKind> {
    match fs::read_dir(dir) {
        Ok(files) => {
            let names_by_id = session_index::session_names_by_id();
            let candidates = files
                .flatten()
                .filter_map(|file| {
                    let id = session_socket_id(&file)?;
                    let name = session_name(&names_by_id, id.clone());
                    Some((id, name, session_age(&file)))
                })
                .collect();
            let observed = observe_sessions_with(candidates, deadline, |id, remaining| {
                assert_socket_in(dir, id, remaining)
            });
            Ok(SessionObservation {
                responsive: observed.responsive,
                unknown: observed.unknown,
            })
        },
        Err(err) if io::ErrorKind::NotFound != err.kind() => Err(err.kind()),
        Err(_) => Ok(SessionObservation::default()),
    }
}

/// Sidecar recording when a session's layout file was first written.
///
/// The layout file is replaced through a temporary file, so its own creation
/// timestamp is that of the latest replacement rather than of the session.
const SESSION_LAYOUT_CREATED_FILE_NAME: &str = "session-layout-created";

fn session_layout_created_marker(layout_file: &Path) -> PathBuf {
    layout_file.with_file_name(SESSION_LAYOUT_CREATED_FILE_NAME)
}

/// Remember when `layout_file` first came into existence, before it is
/// replaced. Does nothing once recorded. A layout file that already exists
/// contributes its own creation (or modification) time; otherwise now.
pub fn record_session_layout_creation_time(layout_file: &Path) {
    let marker = session_layout_created_marker(layout_file);
    if marker.exists() {
        return;
    }
    let created = fs::metadata(layout_file)
        .and_then(|metadata| metadata.created().or_else(|_| metadata.modified()))
        .unwrap_or_else(|_| SystemTime::now());
    let seconds = created
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|since_epoch| since_epoch.as_secs())
        .unwrap_or_default();
    if let Err(error) = fs::write(&marker, seconds.to_string()) {
        log::error!("Failed to record session creation time: {:?}", error);
    }
}

/// When the session owning `layout_file` was created: the recorded time if
/// any, else the layout file's creation (falling back to modification) time.
/// Fails with the metadata error when the layout file does not exist.
pub fn session_layout_creation_time(layout_file: &Path) -> io::Result<SystemTime> {
    let metadata = fs::metadata(layout_file)?;
    if let Some(recorded) = fs::read_to_string(session_layout_created_marker(layout_file))
        .ok()
        .and_then(|contents| contents.trim().parse::<u64>().ok())
    {
        if let Some(created) = SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(recorded)) {
            return Ok(created);
        }
    }
    metadata.created().or_else(|_| metadata.modified())
}

pub fn get_resurrectable_sessions() -> Vec<(String, Duration)> {
    match fs::read_dir(&*ZELLIJ_SESSION_INFO_CACHE_DIR) {
        Ok(files_in_session_info_folder) => {
            let names_by_id = session_index::session_names_by_id();
            let files_that_are_folders = files_in_session_info_folder
                .filter_map(|f| f.ok().map(|f| f.path()))
                .filter(|f| f.is_dir());
            files_that_are_folders
                .filter_map(|folder_name| {
                    let layout_file_name =
                        session_layout_cache_file_name(&folder_name.display().to_string());
                    // Try to get creation time, fall back to modification time on platforms where it's not supported (e.g., musl)
                    let ctime = session_layout_creation_time(&layout_file_name).ok();
                    let elapsed_duration = ctime
                        .map(|ctime| {
                            Duration::from_secs(ctime.elapsed().ok().unwrap_or_default().as_secs())
                        })
                        .unwrap_or_default();
                    let session_id = folder_name
                        .file_name()
                        .map(|f| std::path::PathBuf::from(f).display().to_string())?;
                    if crate::consts::is_app_socket_name(&session_id) {
                        return None;
                    }
                    if layout_file_is_resurrectable(Path::new(&layout_file_name)) {
                        Some((session_name(&names_by_id, session_id), elapsed_duration))
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
            let names_by_id = session_index::session_names_by_id();
            let files_that_are_folders = files_in_session_info_folder
                .filter_map(|f| f.ok().map(|f| f.path()))
                .filter(|f| f.is_dir());
            files_that_are_folders
                .filter_map(|folder_name| {
                    let folder = folder_name.display().to_string();
                    let resurrection_layout_file = session_layout_cache_file_name(&folder);
                    let hidden = folder_name
                        .file_name()
                        .map(|f| crate::consts::is_app_socket_name(&f.to_string_lossy()))
                        .unwrap_or(false);
                    if !hidden && layout_file_is_resurrectable(Path::new(&resurrection_layout_file))
                    {
                        folder_name
                            .file_name()
                            .map(|f| session_name(&names_by_id, f.to_string_lossy().into_owned()))
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
    match fs::read_dir(&*ZELLIJ_SOCK_DIR) {
        Ok(files) => {
            let names_by_id = session_index::session_names_by_id();
            let mut candidates = Vec::new();
            for file in files.flatten() {
                let file_name = match file.file_name().into_string() {
                    Ok(name) => name,
                    Err(_) => continue,
                };
                if crate::consts::is_app_socket_name(&file_name) {
                    continue;
                }
                // Inspection failure is not "no session": callers create a replacement
                // when this list is empty.
                let file_type = file.file_type()?;
                if !is_ipc_socket(&file_type) {
                    continue;
                }
                let modified_at = session_mtime_result(file.metadata())?;
                let name = session_name(&names_by_id, file_name.clone());
                candidates.push((file_name, name, modified_at));
            }
            let mut sessions_with_mtime =
                live_sessions(&ZELLIJ_SOCK_DIR, candidates, SESSION_PROBE_TIMEOUT);
            sessions_with_mtime.sort_by_key(|x| x.1); // the oldest one will be the first

            let sessions = sessions_with_mtime.iter().map(|x| x.0.clone()).collect();
            Ok(sessions)
        },
        Err(err) if io::ErrorKind::NotFound != err.kind() => Err(err.into()),
        Err(_) => Ok(Vec::with_capacity(0)),
    }
}

/// Probe a session socket to check if a server is alive.
///
/// On Unix, connects and sends a `ConnStatus` message to verify the server responds.
/// On Windows, reads the server PID from the marker file and checks process liveness.
///
/// Ordinary probing is observational: a refused connection proves absence for discovery
/// purposes but does **not** unlink the path. Stale-path reclamation belongs exclusively to
/// [`crate::session_socket_claim::SessionSocketClaim::reclaim_stale_socket`].
#[cfg(unix)]
fn assert_socket_in(dir: &Path, name: &str, timeout: Duration) -> SessionProbeResult {
    assert_socket_path(&dir.join(name), timeout)
}

/// Probe the socket at `path` with one absolute deadline covering connect, the
/// `ConnStatus` write, and the whole reply frame.
///
/// A socket read timeout is an inactivity timer, so it is refreshed from that
/// deadline before every read and write. Timeout, short read, hang-up, an
/// oversized length prefix, and any message other than `Connected` are
/// [`SessionProbeResult::Unknown`]. Refused and missing paths stay
/// [`SessionProbeResult::Absent`] and are not unlinked.
#[cfg(unix)]
fn assert_socket_path(path: &Path, timeout: Duration) -> SessionProbeResult {
    use crate::consts::ipc_connect_with_timeout;
    use crate::ipc::{
        encode_client_frame, read_server_frame_until_deadline, write_all_until_deadline,
        PROBE_MAX_FRAME_BYTES,
    };

    if timeout.is_zero() {
        return SessionProbeResult::Unknown;
    }
    let deadline = Instant::now() + timeout;
    let mut stream = match ipc_connect_with_timeout(path, timeout) {
        Ok(stream) => stream,
        // Refused / missing: proven gone for observation. Do not unlink — a concurrent
        // replacement may already own this pathname.
        Err(err)
            if matches!(
                err.kind(),
                io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
            ) =>
        {
            return SessionProbeResult::Absent;
        },
        Err(_) => return SessionProbeResult::Unknown,
    };
    let frame = match encode_client_frame(ClientToServerMsg::ConnStatus) {
        Ok(frame) => frame,
        Err(_) => return SessionProbeResult::Unknown,
    };
    if write_all_until_deadline(&mut stream, &frame, deadline).is_err() {
        return SessionProbeResult::Unknown;
    }
    match read_server_frame_until_deadline(&mut stream, deadline, PROBE_MAX_FRAME_BYTES) {
        Ok(ServerToClientMsg::Connected) => SessionProbeResult::Responsive,
        Ok(_) | Err(_) => SessionProbeResult::Unknown,
    }
}

/// Classification of a Windows `OpenProcess` result for a session marker PID.
///
/// A successful handle only means *some* process currently has that PID — not that it is the
/// recorded Zellij server (PID reuse). Access denied / other failures stay unknown; only a clear
/// no-such-PID error authorises treating the marker as dead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsPidLiveness {
    /// `OpenProcess` returned a handle. Identity with the recorded server is not proven.
    /// The session probe maps this to [`SessionProbeResult::Unknown`], not Responsive.
    AliveUnverified,
    /// Process is gone (`ERROR_INVALID_PARAMETER` after a null handle).
    Dead,
    /// Inspection failed without proving death (access denied, other errors, missing GetLastError).
    Unknown,
}

/// `ERROR_ACCESS_DENIED` from Win32.
pub const WINDOWS_ERROR_ACCESS_DENIED: u32 = 5;
/// `ERROR_INVALID_PARAMETER` from Win32 — documented for `OpenProcess` when the PID does not exist.
pub const WINDOWS_ERROR_INVALID_PARAMETER: u32 = 87;

/// Pure classifier for unit tests and the Windows probe. `last_error` is meaningful only when
/// `handle_is_null` is true. Access denied is not death.
pub fn classify_windows_open_process(handle_is_null: bool, last_error: u32) -> WindowsPidLiveness {
    if !handle_is_null {
        return WindowsPidLiveness::AliveUnverified;
    }
    match last_error {
        WINDOWS_ERROR_INVALID_PARAMETER => WindowsPidLiveness::Dead,
        WINDOWS_ERROR_ACCESS_DENIED => WindowsPidLiveness::Unknown,
        _ => WindowsPidLiveness::Unknown,
    }
}

/// Map a Windows PID check onto a session probe result.
///
/// [`WindowsPidLiveness::AliveUnverified`] is not [`SessionProbeResult::Responsive`]:
/// a reused PID is not proof of the recorded server. [`WindowsPidLiveness::Dead`]
/// stays absent. Access denied and every other non-death result stay unknown.
pub fn probe_result_for_windows_liveness(liveness: WindowsPidLiveness) -> SessionProbeResult {
    match liveness {
        WindowsPidLiveness::AliveUnverified => SessionProbeResult::Unknown,
        WindowsPidLiveness::Dead => SessionProbeResult::Absent,
        WindowsPidLiveness::Unknown => SessionProbeResult::Unknown,
    }
}

/// Liveness of a Windows session marker, for reclaim. Missing or unreadable markers are
/// [`WindowsPidLiveness::Unknown`] so a claim cannot treat them as dead.
pub fn liveness_of_session_marker(path: &Path) -> WindowsPidLiveness {
    #[cfg(windows)]
    {
        let pid_str = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(_) => return WindowsPidLiveness::Unknown,
        };
        return match pid_str.trim().parse() {
            Ok(pid) => windows_open_process_liveness(pid),
            Err(_) => WindowsPidLiveness::Unknown,
        };
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        WindowsPidLiveness::Unknown
    }
}

#[cfg(windows)]
fn windows_open_process_liveness(pid: u32) -> WindowsPidLiveness {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    let (handle_is_null, last_error) = unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            (true, GetLastError())
        } else {
            CloseHandle(handle);
            (false, 0)
        }
    };
    classify_windows_open_process(handle_is_null, last_error)
}

/// On Windows, reads the server PID from the marker file and checks whether
/// the process is still alive via `OpenProcess`. Observation never deletes markers;
/// reclamation is reserved for [`SessionSocketClaim::reclaim_stale_socket`].
#[cfg(windows)]
fn assert_socket_in(dir: &Path, name: &str, timeout: Duration) -> SessionProbeResult {
    assert_socket_path(&dir.join(name), timeout)
}

#[cfg(windows)]
fn assert_socket_path(path: &Path, _timeout: Duration) -> SessionProbeResult {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    let pid_str = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return SessionProbeResult::Absent;
        },
        Err(_) => {
            // Unreadable marker: unknown, do not delete (may be a permissions race).
            return SessionProbeResult::Unknown;
        },
    };
    let pid: u32 = match pid_str.trim().parse() {
        Ok(p) => p,
        Err(_) => {
            // Marker exists but has no valid PID — observationally absent, leave the file.
            return SessionProbeResult::Absent;
        },
    };
    let (handle_is_null, last_error) = unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            (true, GetLastError())
        } else {
            CloseHandle(handle);
            (false, 0)
        }
    };
    probe_result_for_windows_liveness(classify_windows_open_process(handle_is_null, last_error))
}

#[cfg(not(any(unix, windows)))]
fn assert_socket_in(_dir: &Path, _name: &str, _timeout: Duration) -> SessionProbeResult {
    SessionProbeResult::Responsive
}

#[cfg(not(any(unix, windows)))]
fn assert_socket_path(_path: &Path, _timeout: Duration) -> SessionProbeResult {
    SessionProbeResult::Responsive
}

/// Error-preserving path presence check. [`Path::exists`] maps inspection failures to `false`.
fn path_try_exists(path: &Path) -> Result<bool, io::ErrorKind> {
    path.try_exists().map_err(|err| err.kind())
}

/// Presence of a resurrection layout. An inspection error is [`LayoutPresence::Unknown`],
/// not [`LayoutPresence::Absent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutPresence {
    Present,
    Absent,
    Unknown,
}

pub fn layout_presence(try_exists: Result<bool, io::ErrorKind>) -> LayoutPresence {
    match try_exists {
        Ok(true) => LayoutPresence::Present,
        Ok(false) => LayoutPresence::Absent,
        Err(_) => LayoutPresence::Unknown,
    }
}

fn layout_counts_as_resurrectable(presence: LayoutPresence) -> bool {
    !matches!(presence, LayoutPresence::Absent)
}

fn layout_file_is_resurrectable(path: &Path) -> bool {
    layout_counts_as_resurrectable(layout_presence(path_try_exists(path)))
}

/// Metadata used to order sessions. `Err` must not be collapsed into "no session".
fn session_mtime_result(metadata: io::Result<fs::Metadata>) -> anyhow::Result<SystemTime> {
    Ok(metadata?.modified()?)
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
    /// Socket(s) exist but none answered before the probe deadline. Must not be reported as
    /// "no active session".
    Unresponsive(Vec<String>),
}

pub fn get_active_session() -> ActiveSession {
    match observe_sessions() {
        Ok(obs) if obs.is_definitely_empty() => ActiveSession::None,
        Ok(obs) if obs.responsive.is_empty() => {
            ActiveSession::Unresponsive(obs.unknown.into_iter().map(|(name, _)| name).collect())
        },
        Ok(mut obs) if obs.responsive.len() == 1 => {
            ActiveSession::One(obs.responsive.pop().unwrap().0)
        },
        Ok(_) => ActiveSession::Many,
        Err(e) => {
            eprintln!("Error occurred: {:?}", e);
            process::exit(1);
        },
    }
}

/// Filter saved sessions down to those that delete-all-sessions may remove.
///
/// Unresponsive sockets are active for this purpose unless `force` explicitly
/// authorizes killing and deleting them.
pub fn resurrectable_sessions_to_delete(
    force: bool,
    observation: &SessionObservation,
    resurrectable_sessions: Vec<(String, Duration)>,
) -> Vec<(String, Duration)> {
    if force {
        return resurrectable_sessions;
    }
    let active_sessions: std::collections::BTreeSet<&str> = observation
        .responsive
        .iter()
        .chain(observation.unknown.iter())
        .map(|(name, _)| name.as_str())
        .collect();
    resurrectable_sessions
        .into_iter()
        .filter(|(name, _)| !active_sessions.contains(name.as_str()))
        .collect()
}

/// Send `KillSession` on a paired connection.
///
/// On Windows the request and reply pipes are connected together, and the
/// reply is read so the request is not dropped before the server has it.
/// The timeout bounds the connect and the reply read. It does not cancel a
/// write that is already blocked in the pipe.
pub fn send_kill_session(path: &Path, timeout: Duration) -> io::Result<()> {
    #[cfg(windows)]
    {
        use interprocess::local_socket::prelude::*;
        let (stream, reply_stream) = crate::pipe_pairs::connect_pair_with_timeout(path, timeout)?;
        IpcSenderWithContext::<ClientToServerMsg>::new(stream)
            .send_client_msg(ClientToServerMsg::KillSession)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let _ = reply_stream.set_recv_timeout(Some(timeout));
        let mut receiver: IpcReceiverWithContext<ServerToClientMsg> =
            IpcReceiverWithContext::new(reply_stream);
        let _ = receiver.recv_server_msg();
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        let stream = crate::consts::ipc_connect_with_timeout(path, timeout)?;
        IpcSenderWithContext::<ClientToServerMsg>::new(stream)
            .send_client_msg(ClientToServerMsg::KillSession)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        Ok(())
    }
}

fn report_kill_session_error(action: &str, name: &str, error: &io::Error) {
    if matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    ) {
        eprintln!(
            "Timed out while trying to {} session {:?}; the session socket did not respond \
             and its state was left intact.",
            action, name
        );
    } else {
        eprintln!("Failed to {} session {:?}: {}", action, name, error);
    }
}

fn legacy_socket_path(name: &str) -> Result<PathBuf, String> {
    Ok(match resolve_session_socket_path(name)? {
        Some(path) => path,
        None => ZELLIJ_SOCK_DIR.join(name),
    })
}

pub fn kill_session(name: &str) {
    let path = match legacy_socket_path(name) {
        Ok(path) => path,
        Err(error) => {
            eprintln!(
                "Failed to read the session index for {:?}: {}. The session was not killed.",
                name, error
            );
            process::exit(1);
        },
    };
    if let Err(e) = send_kill_session(&path, SESSION_PROBE_TIMEOUT) {
        report_kill_session_error("kill", name, &e);
        process::exit(1);
    }
}

pub fn delete_session(name: &str, force: bool) {
    // The socket and the cache folder are named by the session id; the index row by the name.
    // A failed index read must not delete the folder named by the display name.
    let session_id = match session_folder_id(name) {
        Ok(session_id) => session_id,
        Err(error) => {
            eprintln!(
                "Failed to read the session index for {:?}: {}. The session was not deleted.",
                name, error
            );
            process::exit(1);
        },
    };
    if let Err(e) = validate_session_folder_name(&session_id) {
        eprintln!("{}", e);
        process::exit(1);
    }
    if force {
        let path = ZELLIJ_SOCK_DIR.join(&session_id);
        if let Err(e) = send_kill_session(&path, SESSION_PROBE_TIMEOUT) {
            if !matches!(
                e.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) {
                report_kill_session_error("force-delete", name, &e);
                eprintln!(
                    "Continuing because --force explicitly permits deleting session {:?}.",
                    name
                );
            }
        }
    }
    // Fence and remove under one per-session lock. The index row is removed
    // only after that removal succeeds. A fence failure leaves the row, the
    // directory stays, and the process does not exit 0.
    match remove_fenced_session_cache(&session_id) {
        Ok(SessionCacheRemoval::Removed) => {
            if let Ok(index) = crate::session_index::SessionIndex::open_default() {
                let _ = index.delete(name);
            }
            println!("Session: {:?} successfully deleted.", name);
        },
        Ok(SessionCacheRemoval::NotFound) => {
            eprintln!("Session: {:?} not found.", name);
            process::exit(2);
        },
        Err(error) => {
            eprintln!("Failed to delete session {:?}: {}", name, error);
            log::error!("Failed to delete session {:?}: {}", name, error);
            process::exit(1);
        },
    }
}

/// Combine resurrectable layouts with a live observation for display.
///
/// Unknown (unresponsive) sockets that also have a saved layout must **not** be marked EXITED —
/// a timeout is weaker evidence than process death.
fn sessions_for_display(
    observation: &SessionObservation,
    resurrectable: Vec<(String, Duration)>,
) -> Vec<(String, Duration, bool)> {
    let mut all_sessions: HashMap<String, (Duration, bool)> = resurrectable
        .into_iter()
        .map(|(name, timestamp)| (name, (timestamp, true)))
        .collect();
    for (session_name, duration) in &observation.responsive {
        all_sessions.insert(session_name.clone(), (*duration, false));
    }
    for (session_name, _) in &observation.unknown {
        // Drop any EXITED entry inherited from a layout; the unresponsive footer reports them.
        all_sessions.remove(session_name);
    }
    all_sessions
        .into_iter()
        .map(|(name, (timestamp, is_dead))| (name, timestamp, is_dead))
        .collect()
}

/// Why a publication attempt was refused.
///
/// Returned as the inner error of an [`io::Error`] with
/// [`io::ErrorKind::PermissionDenied`]. A fenced save is never `Ok(())`.
/// Identify it with [`is_session_publication_fenced`]; a plain permission
/// error from the filesystem does not downcast to this type.
#[derive(Debug)]
pub struct SessionPublicationFenced {
    pub session_id: String,
    pub incarnation: u128,
}

impl std::fmt::Display for SessionPublicationFenced {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "session publication fenced for {} incarnation {}",
            self.session_id, self.incarnation
        )
    }
}

impl std::error::Error for SessionPublicationFenced {}

/// `true` when `error` is a fenced publication ([`SessionPublicationFenced`]).
pub fn is_session_publication_fenced(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|inner| inner.is::<SessionPublicationFenced>())
}

fn publication_fenced_error(session_id: &str, incarnation: u128) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        SessionPublicationFenced {
            session_id: session_id.to_owned(),
            incarnation,
        },
    )
}

/// Outcome of [`remove_fenced_session_cache`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionCacheRemoval {
    Removed,
    NotFound,
}

/// Exclusive cross-process lock for one session id.
///
/// The lock file is a sibling of the session directory, so `remove_dir_all`
/// of that directory does not drop the lock. Different session ids lock
/// different files. Hold the guard from the eligibility check through staging
/// and persist, and across the tombstone write plus directory removal.
pub struct SessionPublicationLock {
    #[cfg(unix)]
    _file: nix::fcntl::Flock<fs::File>,
    #[cfg(windows)]
    _file: fs::File,
    #[cfg(not(any(unix, windows)))]
    _file: (),
}

/// Lock file, incarnation file, deletion tombstone, and session folder under `cache_dir`.
fn cache_publication_paths(
    cache_dir: &Path,
    session_id: &str,
) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    (
        cache_dir.join(format!("{session_id}.lock")),
        cache_dir.join(format!("{session_id}.incarnation")),
        cache_dir.join(format!("{session_id}.deleted")),
        cache_dir.join(session_id),
    )
}

/// Block until this process holds the per-session publication lock.
///
/// Fails closed when the lock cannot be acquired. On Windows the lock is
/// `LockFileEx`; there is no silent no-op.
pub fn lock_session_publication(session_id: &str) -> io::Result<SessionPublicationLock> {
    lock_session_publication_at(&session_publication_lock_path(session_id))
}

fn lock_session_publication_at(path: &Path) -> io::Result<SessionPublicationLock> {
    if let Some(parent) = path.parent() {
        // The cache directory, not the session folder. Creating the session
        // folder is what a fenced writer must not do.
        fs::create_dir_all(parent)?;
    }
    let mut options = fs::OpenOptions::new();
    options.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    SessionPublicationLock::from_file(file)
}

impl SessionPublicationLock {
    #[cfg(unix)]
    fn from_file(file: fs::File) -> io::Result<Self> {
        use nix::fcntl::{Flock, FlockArg};
        let file = Flock::lock(file, FlockArg::LockExclusive)
            .map_err(|(_file, errno)| io::Error::from(errno))?;
        Ok(SessionPublicationLock { _file: file })
    }

    #[cfg(windows)]
    fn from_file(file: fs::File) -> io::Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{LockFileEx, LOCKFILE_EXCLUSIVE_LOCK};
        use windows_sys::Win32::System::IO::OVERLAPPED;
        // Synchronous handle: a zeroed OVERLAPPED makes LockFileEx wait.
        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        let locked = unsafe {
            LockFileEx(
                file.as_raw_handle() as _,
                LOCKFILE_EXCLUSIVE_LOCK,
                0,
                u32::MAX,
                u32::MAX,
                &mut overlapped,
            )
        };
        if locked == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(SessionPublicationLock { _file: file })
    }

    #[cfg(not(any(unix, windows)))]
    fn from_file(_file: fs::File) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "session publication lock is unavailable on this platform",
        ))
    }
}

/// Read the incarnation file once, when a writer is created.
///
/// Store the value and pass it to later publication attempts. Re-reading at
/// publish time and adopting a newer value would let a new owner reauthorize
/// a writer that was admitted under the deleted incarnation.
pub fn read_session_incarnation(session_id: &str) -> io::Result<u128> {
    match read_incarnation_file(&session_incarnation_path(session_id))? {
        Some(incarnation) => Ok(incarnation),
        None => Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no publication incarnation for session {session_id}"),
        )),
    }
}

/// Caller must hold [`lock_session_publication`] for `session_id` across this
/// check and the following `create_dir_all` / persist.
///
/// `Ok(())` only when `incarnation` equals the incarnation file and the
/// tombstone does not name it. Otherwise a [`SessionPublicationFenced`] error
/// (`PermissionDenied`). Other IO failures are returned unchanged, never `Ok`.
pub fn ensure_session_publication_allowed(session_id: &str, incarnation: u128) -> io::Result<()> {
    ensure_session_publication_allowed_in(&*ZELLIJ_SESSION_INFO_CACHE_DIR, session_id, incarnation)
}

pub(crate) fn ensure_session_publication_allowed_in(
    cache_dir: &Path,
    session_id: &str,
    incarnation: u128,
) -> io::Result<()> {
    let (_, incarnation_path, tombstone_path, _) = cache_publication_paths(cache_dir, session_id);
    let live = read_incarnation_file(&incarnation_path)?;
    let blocked = tombstone_blocks_path(&tombstone_path, incarnation)?;
    if live != Some(incarnation) || blocked {
        Err(publication_fenced_error(session_id, incarnation))
    } else {
        Ok(())
    }
}

/// Publish a deletion tombstone naming the current live incarnation.
///
/// Holds the per-session publication lock while the tombstone is written.
/// Returns `Err` when the marker cannot be written; callers must not remove
/// the session directory in that case. A missing incarnation file is recorded
/// as incarnation `0` (no admitted writer) and is still a successful fence.
pub fn fence_session_deletion(session_id: &str) -> io::Result<()> {
    let _lock = lock_session_publication(session_id)?;
    write_deletion_tombstone(session_id)
}

/// Fence `session_id` and remove its cache folder while holding the
/// per-session publication lock.
///
/// On fence failure the folder is left in place and the error is returned.
/// If removal fails after a successful fence, the tombstone stays.
pub fn remove_fenced_session_cache(session_id: &str) -> io::Result<SessionCacheRemoval> {
    remove_fenced_session_cache_in(&*ZELLIJ_SESSION_INFO_CACHE_DIR, session_id)
}

/// Fence `session_id` and remove its folder under `cache_dir` while holding that
/// directory's publication lock. On fence failure the folder stays.
///
/// Tombstones whatever incarnation is current at lock time. A caller that
/// selected this session earlier must pass that lifetime to
/// [`remove_fenced_session_cache_for_lifetime`] instead. The lock serializes
/// the write, but it does not keep the earlier death decision.
pub(crate) fn remove_fenced_session_cache_in(
    cache_dir: &Path,
    session_id: &str,
) -> io::Result<SessionCacheRemoval> {
    let (lock_path, incarnation_path, tombstone_path, folder) =
        cache_publication_paths(cache_dir, session_id);
    let _lock = lock_session_publication_at(&lock_path)?;
    fence_and_remove_locked(&incarnation_path, &tombstone_path, &folder)
}

/// Incarnation observed when a session was classified dead.
///
/// `Absent` is a missing incarnation file, not "any incarnation". A file that
/// appears later is a different lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectedSessionLifetime {
    Absent,
    Present(u128),
}

pub(crate) fn selected_session_lifetime(
    cache_dir: &Path,
    session_id: &str,
) -> io::Result<SelectedSessionLifetime> {
    let (_, incarnation_path, _, _) = cache_publication_paths(cache_dir, session_id);
    match read_incarnation_file(&incarnation_path)? {
        Some(incarnation) => Ok(SelectedSessionLifetime::Present(incarnation)),
        None => Ok(SelectedSessionLifetime::Absent),
    }
}

fn selected_lifetime_is_current(selected: SelectedSessionLifetime, current: Option<u128>) -> bool {
    match (selected, current) {
        (SelectedSessionLifetime::Absent, None) => true,
        (SelectedSessionLifetime::Present(selected), Some(current)) => selected == current,
        _ => false,
    }
}

#[derive(Debug)]
struct SelectedLifetimeMismatch {
    session_id: String,
}

impl std::fmt::Display for SelectedLifetimeMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "session {} changed incarnation after it was selected for deletion",
            self.session_id
        )
    }
}

impl std::error::Error for SelectedLifetimeMismatch {}

pub(crate) fn is_selected_lifetime_mismatch(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|inner| inner.is::<SelectedLifetimeMismatch>())
}

fn selected_lifetime_mismatch(session_id: &str) -> io::Error {
    io::Error::other(SelectedLifetimeMismatch {
        session_id: session_id.to_owned(),
    })
}

/// Fence and remove only if the incarnation under the publication lock is still
/// `selected`. A newer incarnation is not tombstoned and its folder stays.
///
/// The comparison runs after the lock is held. An earlier unlocked read is not
/// enough: admission writes the new incarnation under the same lock.
pub(crate) fn remove_fenced_session_cache_for_lifetime(
    cache_dir: &Path,
    session_id: &str,
    selected: SelectedSessionLifetime,
) -> io::Result<SessionCacheRemoval> {
    let (lock_path, incarnation_path, tombstone_path, folder) =
        cache_publication_paths(cache_dir, session_id);
    let _lock = lock_session_publication_at(&lock_path)?;
    let current = read_incarnation_file(&incarnation_path)?;
    if !selected_lifetime_is_current(selected, current) {
        return Err(selected_lifetime_mismatch(session_id));
    }
    fence_and_remove_locked(&incarnation_path, &tombstone_path, &folder)
}

fn fence_and_remove_locked(
    incarnation_path: &Path,
    tombstone_path: &Path,
    folder: &Path,
) -> io::Result<SessionCacheRemoval> {
    write_deletion_tombstone_at(incarnation_path, tombstone_path)?;
    match fs::remove_dir_all(folder) {
        Ok(()) => Ok(SessionCacheRemoval::Removed),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(SessionCacheRemoval::NotFound),
        Err(error) => Err(error),
    }
}

fn session_is_not_definitely_dead(session_id: &str) -> io::Error {
    io::Error::other(format!(
        "session {session_id} is not definitely dead; its cache and index row were left in place"
    ))
}

/// Take the session socket claim. The caller holds it across the death check,
/// incarnation read, and fenced delete, then releases it.
///
/// `Ok(None)` means another owner already holds the claim. That session is not
/// dead: server bind takes this claim before it can listen or admit a new
/// incarnation. `AddrInUse` must not be treated as death.
pub(crate) fn try_acquire_session_socket_claim(
    sock_dir: &Path,
    session_id: &str,
) -> io::Result<Option<crate::session_socket_claim::SessionSocketClaim>> {
    let socket_path = sock_dir.join(session_id);
    match crate::session_socket_claim::SessionSocketClaim::try_acquire(&socket_path) {
        Ok(claim) => Ok(Some(claim)),
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => Ok(None),
        Err(error) => Err(error),
    }
}

/// Remove `session_id`'s cache only when a socket inventory says it is dead.
///
/// The socket claim is held across the death check, the incarnation read, and
/// the fenced delete. A resurrection has to take that claim before it can bind,
/// so it cannot admit a new incarnation into the lifetime this function reads.
/// The same inventory as bulk deletion: an unreadable directory yields no dead
/// ids, so this refuses. The incarnation read with that decision is checked
/// again under the publication lock.
pub fn remove_fenced_dead_session_cache(
    sock_dir: &Path,
    cache_dir: &Path,
    session_id: &str,
) -> io::Result<SessionCacheRemoval> {
    let Some(claim) = try_acquire_session_socket_claim(sock_dir, session_id)? else {
        return Err(session_is_not_definitely_dead(session_id));
    };
    let removal = remove_fenced_dead_session_cache_while_claimed(sock_dir, cache_dir, session_id);
    claim.release();
    removal
}

fn remove_fenced_dead_session_cache_while_claimed(
    sock_dir: &Path,
    cache_dir: &Path,
    session_id: &str,
) -> io::Result<SessionCacheRemoval> {
    if !crate::session_index::session_id_is_definitely_dead(sock_dir, cache_dir, session_id) {
        return Err(session_is_not_definitely_dead(session_id));
    }
    let selected = selected_session_lifetime(cache_dir, session_id)?;
    remove_fenced_session_cache_for_lifetime(cache_dir, session_id, selected)
}

/// Whether a deletion tombstone is present.
///
/// [`SessionDeletionFence::Unreadable`] is not "unfenced": a metadata error must not
/// authorize a new publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionDeletionFence {
    Fenced,
    Unfenced,
    Unreadable,
}

pub fn deletion_fence_from_try_exists(
    try_exists: Result<bool, io::ErrorKind>,
) -> SessionDeletionFence {
    match try_exists {
        Ok(true) => SessionDeletionFence::Fenced,
        Ok(false) => SessionDeletionFence::Unfenced,
        Err(_) => SessionDeletionFence::Unreadable,
    }
}

/// Inspect the deletion tombstone. An unreadable path is [`SessionDeletionFence::Unreadable`],
/// not [`SessionDeletionFence::Unfenced`].
pub fn session_deletion_is_fenced(session_id: &str) -> SessionDeletionFence {
    deletion_fence_from_try_exists(path_try_exists(&session_deletion_tombstone_path(
        session_id,
    )))
}

/// Admit a new incarnation and drop any deletion tombstone.
///
/// Called before writers are started. A writer that already captured the
/// previous incarnation stays refused after this returns, because publication
/// compares that stored value to the file. Failures are logged; this still
/// returns `()`.
pub fn clear_session_deletion_fence(session_id: &str) {
    if let Err(error) = admit_session_incarnation(session_id) {
        log::error!(
            "Failed to clear session deletion fence for {}: {}",
            session_id,
            error
        );
    }
}

fn admit_session_incarnation(session_id: &str) -> io::Result<()> {
    admit_session_incarnation_in(&*ZELLIJ_SESSION_INFO_CACHE_DIR, session_id).map(|_| ())
}

pub(crate) fn admit_session_incarnation_in(cache_dir: &Path, session_id: &str) -> io::Result<u128> {
    let (lock_path, incarnation_path, tombstone_path, _) =
        cache_publication_paths(cache_dir, session_id);
    let _lock = lock_session_publication_at(&lock_path)?;
    let incarnation = mint_session_incarnation();
    write_sibling_atomically(&incarnation_path, format!("{incarnation}\n").as_bytes())?;
    match fs::remove_file(tombstone_path) {
        Ok(()) => Ok(incarnation),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(incarnation),
        Err(error) => Err(error),
    }
}

fn mint_session_incarnation() -> u128 {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let nanos = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let tick = COUNTER.fetch_add(1, Ordering::Relaxed) as u128;
    let pid = std::process::id() as u128;
    // Time in the high half, pid and a per-process counter in the low half.
    // The counter keeps two claims in the same nanosecond distinct.
    let low = (pid << 32) | (tick & 0xffff_ffff);
    let value = ((nanos as u64) as u128) << 64 | low;
    if value == 0 {
        1
    } else {
        value
    }
}

fn read_incarnation_file(path: &Path) -> io::Result<Option<u128>> {
    match fs::read_to_string(path) {
        Ok(text) => {
            let text = text.trim();
            if text.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "empty session incarnation file",
                ));
            }
            text.parse::<u128>().map(Some).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid session incarnation: {error}"),
                )
            })
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn tombstone_blocks_path(path: &Path, incarnation: u128) -> io::Result<bool> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    // A tombstone that does not name an incarnation (older marker, or a
    // truncated write) fences every writer. A named one fences only that
    // incarnation.
    match named_tombstone_incarnation(&text) {
        Some(named) => Ok(named == incarnation),
        None => Ok(true),
    }
}

fn named_tombstone_incarnation(text: &str) -> Option<u128> {
    for line in text.lines() {
        if let Some(value) = line.trim().strip_prefix("incarnation=") {
            return value.trim().parse().ok();
        }
    }
    None
}

fn write_deletion_tombstone(session_id: &str) -> io::Result<()> {
    write_deletion_tombstone_at(
        &session_incarnation_path(session_id),
        &session_deletion_tombstone_path(session_id),
    )
}

fn write_deletion_tombstone_at(incarnation_path: &Path, tombstone_path: &Path) -> io::Result<()> {
    let incarnation = read_incarnation_file(incarnation_path)?.unwrap_or(0);
    let contents = format!("deleted\nincarnation={incarnation}\n");
    write_sibling_atomically(tombstone_path, contents.as_bytes())
}

/// Atomic replace of a cache-directory sibling. Does not create the session folder.
fn write_sibling_atomically(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "session publication path has no parent directory",
            )
        })?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".zellij-session-meta-")
        .tempfile_in(parent)?;
    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

pub fn list_sessions(no_formatting: bool, short: bool, reverse: bool) {
    let exit_code = match observe_sessions() {
        Ok(observation) => {
            let all_sessions = sessions_for_display(&observation, get_resurrectable_sessions());
            if all_sessions.is_empty() && observation.unknown.is_empty() {
                eprintln!("No active zellij sessions found.");
                1
            } else if all_sessions.is_empty() {
                // Unknown sockets must not become a successful empty inventory.
                eprintln!(
                    "Found session socket(s) that did not respond in time; \
                     cannot claim there are no active sessions:"
                );
                for (name, _) in &observation.unknown {
                    eprintln!("  {} (unresponsive)", name);
                }
                1
            } else {
                print_sessions(all_sessions, no_formatting, short, reverse);
                if !observation.unknown.is_empty() {
                    eprintln!("Also found session socket(s) that did not respond in time:");
                    for (name, _) in &observation.unknown {
                        eprintln!("  {} (unresponsive)", name);
                    }
                }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionNameMatch {
    AmbiguousPrefix(Vec<String>),
    UniquePrefix(String),
    Exact(String),
    None,
}

/// Decide an exact name from the direct probe, then prefix-match the listing.
///
/// `Responsive` is [`SessionNameMatch::Exact`] even when `responsive` omitted the name.
/// `Unknown` is [`io::ErrorKind::TimedOut`], never [`SessionNameMatch::None`].
/// `Absent` may prefix-match `responsive`, but an unknown candidate that shares the prefix
/// stays in the ambiguous set (or times out when it is the only candidate).
pub fn decide_session_name_match(
    prefix: &str,
    responsive: &[(String, Duration)],
    unknown: &[(String, Duration)],
    direct: SessionProbeResult,
) -> Result<SessionNameMatch, io::ErrorKind> {
    match direct {
        SessionProbeResult::Responsive => {
            return Ok(SessionNameMatch::Exact(prefix.to_owned()));
        },
        SessionProbeResult::Unknown => return Err(io::ErrorKind::TimedOut),
        SessionProbeResult::Absent => {},
    }

    let mut matches = Vec::new();
    for (name, _) in responsive {
        // The direct probe already proved this exact session is gone.
        if name == prefix {
            continue;
        }
        if name.starts_with(prefix) {
            matches.push(name.clone());
        }
    }
    let mut saw_unknown = false;
    for (name, _) in unknown {
        if name.starts_with(prefix) && !matches.iter().any(|existing| existing == name) {
            saw_unknown = true;
            matches.push(name.clone());
        }
    }
    if saw_unknown && matches.len() == 1 {
        return Err(io::ErrorKind::TimedOut);
    }
    if saw_unknown {
        return Ok(SessionNameMatch::AmbiguousPrefix(matches));
    }
    Ok(match matches.as_slice() {
        [] => SessionNameMatch::None,
        [one] => SessionNameMatch::UniquePrefix(one.clone()),
        _ => SessionNameMatch::AmbiguousPrefix(matches),
    })
}

pub fn match_session_name(prefix: &str) -> Result<SessionNameMatch, io::Error> {
    let observation = observe_sessions().map_err(io::Error::from)?;
    let direct = probe_exact_session(prefix)?;
    decide_session_name_match(
        prefix,
        &observation.responsive,
        &observation.unknown,
        direct,
    )
    .map_err(io::Error::from)
}

fn probe_exact_session(name: &str) -> Result<SessionProbeResult, io::Error> {
    // Exact-name/ID lookups probe the known target directly. Waiting on an enumeration of
    // unrelated candidates can exhaust the shared deadline before this socket is examined.
    // An index error must not probe the display-name path.
    let socket_path =
        legacy_socket_path(name).map_err(|error| io::Error::new(io::ErrorKind::Other, error))?;
    match path_try_exists(&socket_path).map_err(io::Error::from)? {
        false => Ok(SessionProbeResult::Absent),
        true => Ok(assert_socket_path(&socket_path, SESSION_PROBE_TIMEOUT)),
    }
}

pub fn session_exists(name: &str) -> Result<bool, io::Error> {
    match probe_exact_session(name)? {
        SessionProbeResult::Responsive => Ok(true),
        SessionProbeResult::Absent => Ok(false),
        // A socket that exists but could not be confirmed live is unknown, not absent.
        SessionProbeResult::Unknown => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "session socket did not respond before the probe deadline",
        )),
    }
}

// if the session is resurrecable, the returned layout is the one to be used to resurrect it
pub fn resurrection_layout(session_name_to_resurrect: &str) -> Result<Option<Layout>, String> {
    let session_id = session_folder_id(session_name_to_resurrect)?;
    let layout_file_name = session_layout_cache_file_name(&session_id);
    resurrection_layout_from_path_with_reader_barrier(&layout_file_name, || {})
}

/// Read a saved layout together with every pane-content file it references.
///
/// Content files are content-addressed and may be reclaimed after later saves.
/// A reader can therefore observe a layout whose files were pruned while it was
/// parsing. The parser represents a missing file as absent pane contents, so
/// compare the number of references with the number of loaded contents. On a
/// mismatch, restart from the currently committed layout rather than returning
/// a silently incomplete resurrection.
///
/// `after_layout_read` is a test seam for blocking a real reader between the
/// layout read and the content loads.
#[doc(hidden)]
pub fn resurrection_layout_from_path_with_reader_barrier<F>(
    layout_file_name: &Path,
    mut after_layout_read: F,
) -> Result<Option<Layout>, String>
where
    F: FnMut(),
{
    let read_layout = || {
        let raw_layout = fs::read_to_string(layout_file_name).map_err(|e| {
            crate::input::config::ConfigError::IoPath(e, layout_file_name.to_path_buf())
        })?;
        after_layout_read();
        Ok((
            Some(layout_file_name.display().to_string()),
            raw_layout,
            None,
        ))
    };
    match crate::input::layout::read_layout_with_verified_contents(read_layout) {
        Ok((layout, _raw_layout)) => Ok(Some(layout)),
        Err(crate::input::config::ConfigError::IoPath(e, _))
            if e.kind() == io::ErrorKind::NotFound =>
        {
            Ok(None)
        },
        Err(e) => {
            let error_text = crate::input::keybind_presets::config_error_text(&e);
            log::error!(
                "Failed to read resurrection layout file {}: {}",
                layout_file_name.display(),
                error_text
            );
            Err(format!(
                "Failed to read resurrection layout file {}: {}.",
                layout_file_name.display(),
                error_text
            ))
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
                    .unwrap_or_default()
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
            eprintln!("Error occurred: {}", e);
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
        Err(error) => {
            if error.kind() != io::ErrorKind::TimedOut && error.get_ref().is_some() {
                eprintln!(
                    "Failed to read the session index for {:?}: {}. The session was not deleted.",
                    name, error
                );
            } else {
                eprintln!("Error occurred: {}", error);
            }
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
    #[cfg(windows)]
    if name.contains('\\') {
        return Err("Session name cannot contain '\\'.".to_string());
    }
    Ok(())
}

pub fn validate_session_folder_name(name: &str) -> Result<(), String> {
    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(std::path::Component::Normal(component)), None)
            if component == std::ffi::OsStr::new(name) =>
        {
            Ok(())
        },
        _ => Err(format!("Invalid session name or id: {:?}.", name)),
    }
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
        Err(e) => eprintln!("Error occurred: {}", e),
    };
    process::exit(1);
}

pub fn session_listing_error_message(kind: io::ErrorKind) -> String {
    if kind == io::ErrorKind::TimedOut {
        return "A session socket exists but did not respond before the probe deadline.\n\
                Refusing to treat it as absent or to create a replacement session.\n\
                Re-try once the session responds, or inspect it with `zellij list-sessions`."
            .to_owned();
    }
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
    read_live_session_states_with_timeout(
        current_session_name,
        sock_dir,
        session_info_cache_dir,
        SESSION_PROBE_TIMEOUT,
    )
}

#[doc(hidden)]
pub fn read_live_session_states_with_timeout(
    current_session_name: &str,
    sock_dir: &Path,
    session_info_cache_dir: &Path,
    timeout: Duration,
) -> BTreeMap<String, SessionInfo> {
    let names_by_id = session_index::session_names_by_id();
    let candidates = fs::read_dir(sock_dir)
        .map(|files| {
            files
                .flatten()
                .filter_map(|file| {
                    let id = session_socket_id(&file)?;
                    let name = session_name(&names_by_id, id.clone());
                    let age = session_age(&file);
                    Some((id.clone(), name, (id, age)))
                })
                .collect()
        })
        .unwrap_or_default();
    let observed = observe_sessions_with(candidates, timeout, |id, remaining| {
        assert_socket_in(sock_dir, id, remaining)
    });

    let mut session_infos_on_machine = BTreeMap::new();
    for (name, (id, creation_time)) in observed.responsive.into_iter().chain(observed.unknown) {
        if session_infos_on_machine.contains_key(&name) {
            continue;
        }
        let session_cache_file_name = session_info_cache_dir.join(id).join("session-metadata.kdl");
        let mut session_info = fs::read_to_string(&session_cache_file_name)
            .ok()
            .and_then(|raw| SessionInfo::from_string(&raw, current_session_name).ok())
            .unwrap_or_else(|| SessionInfo::new(name.clone()));
        session_info.name = name.clone();
        session_info.is_current_session = name == current_session_name;
        session_info.creation_time = creation_time;
        session_infos_on_machine.insert(name, session_info);
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
mod session_id_tests {
    use super::*;

    #[test]
    fn generated_id_is_short_hex() {
        let id = generate_session_id();
        assert_eq!(id.len(), crate::consts::SESSION_ID_LENGTH);
        assert!(
            id.bytes().all(|b| b.is_ascii_hexdigit()),
            "id should be hex: {}",
            id
        );
        assert_ne!(id, generate_session_id());
    }
}

#[cfg(test)]
mod session_folder_name_tests {
    use super::*;

    #[test]
    fn session_folder_name_accepts_plain_names() {
        assert!(validate_session_folder_name(&generate_session_id()).is_ok());
        assert!(validate_session_folder_name("my-session").is_ok());
        assert!(validate_session_folder_name("my session.v2").is_ok());
    }

    #[test]
    fn session_folder_name_rejects_paths() {
        for name in [
            "",
            ".",
            "..",
            "../x",
            "../../../../poc",
            "a/b",
            "/abs",
            "a/",
            "./a",
        ] {
            assert!(
                validate_session_folder_name(name).is_err(),
                "should reject {:?}",
                name
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn session_folder_name_rejects_windows_paths() {
        for name in ["..\\x", "a\\b", "C:\\x", "C:x", "\\\\server\\share"] {
            assert!(
                validate_session_folder_name(name).is_err(),
                "should reject {:?}",
                name
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn validate_session_name_rejects_backslash_on_windows() {
        assert!(validate_session_name("a\\b").is_err());
    }
}

#[cfg(test)]
mod deletion_fence_tests {
    use super::*;
    use crate::consts::session_info_folder_for_session;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Instant;

    fn unique_session_id(label: &str) -> String {
        format!(
            "{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    fn remove_sidecars(session_id: &str) {
        let _ = fs::remove_file(session_deletion_tombstone_path(session_id));
        let _ = fs::remove_file(session_incarnation_path(session_id));
        let _ = fs::remove_file(session_publication_lock_path(session_id));
        let _ = fs::remove_dir_all(session_info_folder_for_session(session_id));
    }

    #[test]
    fn deletion_tombstone_fences_writers_and_survives_folder_removal() {
        let session_id = unique_session_id("tombstone-test");
        let folder = session_info_folder_for_session(&session_id);
        remove_sidecars(&session_id);
        clear_session_deletion_fence(&session_id);
        let incarnation = read_session_incarnation(&session_id).unwrap();
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("session-metadata.kdl"), "session").unwrap();

        assert_eq!(
            session_deletion_is_fenced(&session_id),
            SessionDeletionFence::Unfenced
        );
        assert_eq!(
            remove_fenced_session_cache(&session_id).unwrap(),
            SessionCacheRemoval::Removed
        );
        assert_eq!(
            session_deletion_is_fenced(&session_id),
            SessionDeletionFence::Fenced
        );
        let tombstone = fs::read_to_string(session_deletion_tombstone_path(&session_id)).unwrap();
        assert!(
            tombstone.contains(&format!("incarnation={incarnation}")),
            "tombstone must name the live incarnation, got {tombstone}"
        );
        assert_eq!(
            session_deletion_is_fenced(&session_id),
            SessionDeletionFence::Fenced,
            "tombstone must survive remove_dir_all of the session folder"
        );
        assert!(!folder.exists(), "session folder itself must still be gone");

        clear_session_deletion_fence(&session_id);
        assert_eq!(
            session_deletion_is_fenced(&session_id),
            SessionDeletionFence::Unfenced
        );
        let renewed = read_session_incarnation(&session_id).unwrap();
        assert_ne!(renewed, incarnation);
        remove_sidecars(&session_id);
    }

    #[test]
    fn fence_failure_leaves_the_session_directory_in_place() {
        let session_id = unique_session_id("fence-fail");
        let folder = session_info_folder_for_session(&session_id);
        let tombstone = session_deletion_tombstone_path(&session_id);
        remove_sidecars(&session_id);
        clear_session_deletion_fence(&session_id);
        fs::create_dir_all(&folder).unwrap();
        let metadata = folder.join("session-metadata.kdl");
        fs::write(&metadata, "keep").unwrap();
        // A directory where the tombstone file must be written is a real IO failure.
        fs::create_dir(&tombstone).unwrap();

        let fence_error = fence_session_deletion(&session_id).unwrap_err();
        assert!(!is_session_publication_fenced(&fence_error));
        let removal_error = remove_fenced_session_cache(&session_id).unwrap_err();
        assert!(!is_session_publication_fenced(&removal_error));
        assert_eq!(fs::read_to_string(&metadata).unwrap(), "keep");
        assert!(folder.exists(), "fence failure must not delete the session");

        let _ = fs::remove_dir_all(&tombstone);
        remove_sidecars(&session_id);
    }

    #[test]
    fn publication_fence_locks_are_per_session() {
        let first = unique_session_id("lock-a");
        let second = unique_session_id("lock-b");
        remove_sidecars(&first);
        remove_sidecars(&second);
        let arrived = std::sync::Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for session_id in [first.clone(), second.clone()] {
            let arrived = std::sync::Arc::clone(&arrived);
            handles.push(thread::spawn(move || {
                let _lock = lock_session_publication(&session_id)
                    .unwrap_or_else(|error| panic!("lock {session_id}: {error}"));
                arrived.fetch_add(1, Ordering::SeqCst);
                let started = Instant::now();
                while arrived.load(Ordering::SeqCst) < 2 {
                    if started.elapsed() > Duration::from_secs(3) {
                        panic!("two session ids serialized on one publication lock");
                    }
                    thread::sleep(Duration::from_millis(10));
                }
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        remove_sidecars(&first);
        remove_sidecars(&second);
    }

    #[test]
    fn deletion_fence_inspection_error_is_not_unfenced() {
        assert_eq!(
            deletion_fence_from_try_exists(Err(io::ErrorKind::PermissionDenied)),
            SessionDeletionFence::Unreadable
        );
        assert_ne!(
            deletion_fence_from_try_exists(Err(io::ErrorKind::PermissionDenied)),
            SessionDeletionFence::Unfenced
        );
        assert_eq!(
            deletion_fence_from_try_exists(Ok(false)),
            SessionDeletionFence::Unfenced
        );
        assert_eq!(
            deletion_fence_from_try_exists(Ok(true)),
            SessionDeletionFence::Fenced
        );
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_tombstone_does_not_authorize_publication() {
        use std::os::unix::fs::PermissionsExt;

        let session_id = unique_session_id("unreadable-tomb");
        remove_sidecars(&session_id);
        clear_session_deletion_fence(&session_id);
        let incarnation = read_session_incarnation(&session_id).unwrap();
        let tombstone = session_deletion_tombstone_path(&session_id);
        fs::write(&tombstone, format!("deleted\nincarnation={incarnation}\n")).unwrap();
        let mut perms = fs::metadata(&tombstone).unwrap().permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&tombstone, perms).unwrap();
        if fs::read_to_string(&tombstone).is_ok() {
            remove_sidecars(&session_id);
            return;
        }
        assert_ne!(
            session_deletion_is_fenced(&session_id),
            SessionDeletionFence::Unfenced
        );
        let error = ensure_session_publication_allowed(&session_id, incarnation).unwrap_err();
        assert_ne!(error.kind(), io::ErrorKind::NotFound);
        let mut perms = fs::metadata(&tombstone).unwrap().permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&tombstone, perms).unwrap();
        remove_sidecars(&session_id);
    }

    #[test]
    fn bulk_delete_publication_fence_blocks_layout_recreation() {
        let session_id = unique_session_id("bulk-fence");
        let root = tempfile::tempdir().unwrap();
        let sock_dir = root.path().join("sock");
        let cache_dir = root.path().join("cache");
        fs::create_dir_all(&sock_dir).unwrap();
        fs::create_dir_all(&cache_dir).unwrap();
        let incarnation = admit_session_incarnation_in(&cache_dir, &session_id).unwrap();
        let folder = cache_dir.join(&session_id);
        fs::create_dir_all(&folder).unwrap();
        let layout = folder.join("session-layout.kdl");
        fs::write(&layout, b"layout { }\n").unwrap();

        let deleted =
            crate::session_index::delete_dead_sessions_in(&sock_dir, &cache_dir, None).unwrap();
        assert!(deleted >= 1, "bulk delete should remove the dead session");
        assert!(
            !layout.exists(),
            "deleted layout must be gone before republish"
        );
        assert!(
            cache_dir.join(format!("{session_id}.deleted")).is_file(),
            "bulk delete must write the tombstone publication checks"
        );

        let publish = ensure_session_publication_allowed_in(&cache_dir, &session_id, incarnation);
        match &publish {
            Ok(()) => {
                fs::create_dir_all(&folder).unwrap();
                fs::write(&layout, b"layout { }\n").unwrap();
            },
            Err(error) => {
                assert!(
                    is_session_publication_fenced(error),
                    "late writer must see the fenced error, got {error}"
                );
            },
        }
        assert!(
            publish.is_err(),
            "publication of a deleted incarnation must fail"
        );
        assert!(
            !layout.exists(),
            "fenced publication must not recreate the layout"
        );
    }
}

#[cfg(test)]
mod session_entry_tests {
    #[cfg(unix)]
    use super::*;

    #[cfg(unix)]
    #[test]
    fn non_utf8_and_non_socket_directory_entries_are_skipped() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let directory = tempfile::tempdir().unwrap();
        // Linux accepts this name. APFS rejects it (illegal byte sequence) and
        // cannot store a non-UTF-8 name another way; that platform still has to
        // prove a regular file is not a session socket.
        let non_utf8 = directory.path().join(OsString::from_vec(vec![b'n', 0xff]));
        let created_non_utf8 = fs::write(&non_utf8, b"").is_ok();
        fs::write(directory.path().join("regular-file"), b"").unwrap();

        if created_non_utf8 {
            for entry in fs::read_dir(directory.path()).unwrap() {
                let entry = entry.unwrap();
                assert!(session_socket_id(&entry).is_none());
            }
        } else {
            let regular = fs::read_dir(directory.path())
                .unwrap()
                .find_map(|entry| {
                    let entry = entry.unwrap();
                    (entry.file_name() == "regular-file").then_some(entry)
                })
                .unwrap();
            assert!(session_socket_id(&regular).is_none());
        }
    }
}

#[cfg(test)]
mod session_probe_tests {
    use super::*;
    use std::thread;

    #[test]
    fn results_keep_candidate_order_when_probes_finish_out_of_order() {
        let candidates = vec![
            ("slow".to_owned(), "slow".to_owned(), ()),
            ("fast".to_owned(), "fast".to_owned(), ()),
        ];

        let live = observe_sessions_with(candidates, Duration::from_secs(2), |id, _| {
            if id == "slow" {
                thread::sleep(Duration::from_millis(300));
            }
            SessionProbeResult::Responsive
        })
        .responsive;

        let names: Vec<&str> = live.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, vec!["slow", "fast"]);
    }

    #[test]
    fn a_wedged_socket_does_not_hide_a_live_one() {
        let candidates = vec![
            ("wedged".to_owned(), "wedged".to_owned(), ()),
            ("live".to_owned(), "live".to_owned(), ()),
        ];

        let started = Instant::now();
        let observed =
            observe_sessions_with(candidates, Duration::from_millis(500), |id, remaining| {
                if id == "wedged" {
                    thread::sleep(remaining);
                    SessionProbeResult::Unknown
                } else {
                    SessionProbeResult::Responsive
                }
            });

        assert_eq!(observed.responsive.len(), 1);
        assert_eq!(observed.responsive[0].0, "live");
        assert_eq!(observed.unknown.len(), 1);
        assert_eq!(observed.unknown[0].0, "wedged");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "probing took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn many_wedged_sockets_finish_within_the_overall_deadline() {
        let candidates: Vec<(String, String, ())> = (0..12)
            .map(|index| {
                let name = format!("wedged-{index}");
                (name.clone(), name, ())
            })
            .collect();

        let started = Instant::now();
        let observed =
            observe_sessions_with(candidates, Duration::from_millis(400), |_, remaining| {
                // A wedged socket spends its whole budget waiting for a reply.
                thread::sleep(remaining);
                SessionProbeResult::Unknown
            });
        let elapsed = started.elapsed();

        assert!(observed.responsive.is_empty());
        assert_eq!(observed.unknown.len(), 12);
        // Sequentially this would take twelve probe timeouts.
        assert!(
            elapsed < Duration::from_secs(2),
            "probing twelve wedged sockets took {:?}",
            elapsed
        );
    }

    #[test]
    fn alive_but_slow_server_is_unknown_not_absent() {
        let candidates = vec![("slow".to_owned(), "slow".to_owned(), ())];
        let observed =
            observe_sessions_with(candidates, Duration::from_millis(50), |_, remaining| {
                thread::sleep(remaining + Duration::from_millis(20));
                SessionProbeResult::Responsive
            });
        // The probe itself overran; classification still depends on what the probe returns.
        // Explicit unknown from deadline exhaustion is covered below.
        let _ = observed;
        let candidates = vec![("slow".to_owned(), "slow".to_owned(), ())];
        let observed = observe_sessions_with(candidates, Duration::from_millis(1), |_, _| {
            thread::sleep(Duration::from_millis(30));
            SessionProbeResult::Unknown
        });
        assert!(observed.responsive.is_empty());
        assert_eq!(observed.unknown.len(), 1);
        assert_eq!(observed.unknown[0].0, "slow");
    }

    #[test]
    fn eight_slow_candidates_still_surface_a_healthy_ninth_past_the_shared_deadline() {
        // Nine candidates with eight workers: the healthy session is last in the queue, so it is
        // only claimed after a slow worker frees a slot. Slow probes return promptly as Unknown
        // (simulating a timed-out ConnStatus) so the healthy ninth still runs inside the shared
        // deadline — whereas sequential probing would spend eight full timeouts first.
        let mut candidates: Vec<(String, String, ())> = (0..8)
            .map(|index| {
                let name = format!("slow-{index}");
                (name.clone(), name, ())
            })
            .collect();
        candidates.push(("healthy".to_owned(), "healthy".to_owned(), ()));

        let started = Instant::now();
        let observed =
            observe_sessions_with(candidates, Duration::from_millis(500), |id, _remaining| {
                if id == "healthy" {
                    SessionProbeResult::Responsive
                } else {
                    thread::sleep(Duration::from_millis(40));
                    SessionProbeResult::Unknown
                }
            });
        let elapsed = started.elapsed();

        assert_eq!(
            observed
                .responsive
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>(),
            vec!["healthy"]
        );
        assert_eq!(observed.unknown.len(), 8);
        assert!(
            elapsed < Duration::from_millis(400),
            "parallel probing took {:?}, expected well under eight sequential timeouts",
            elapsed
        );
    }

    #[test]
    fn candidates_never_reached_before_the_deadline_are_unknown_not_omitted_as_absent() {
        // Saturate all workers with blockers that consume the shared budget so a trailing
        // candidate is never probed in time. It must be recorded as Unknown, not dropped as Absent.
        let mut candidates: Vec<(String, String, ())> = (0..MAX_CONCURRENT_SESSION_PROBES)
            .map(|index| {
                let name = format!("blocker-{index}");
                (name.clone(), name, ())
            })
            .collect();
        candidates.push(("late".to_owned(), "late".to_owned(), ()));

        let observed =
            observe_sessions_with(candidates, Duration::from_millis(80), |id, remaining| {
                if id == "late" {
                    SessionProbeResult::Responsive
                } else {
                    thread::sleep(remaining);
                    SessionProbeResult::Unknown
                }
            });

        assert!(
            observed.responsive.is_empty(),
            "late candidate should not have been probed as responsive: {:?}",
            observed.responsive
        );
        assert!(
            observed.unknown.iter().any(|(name, _)| name == "late"),
            "unprobed late candidate must stay unknown, got {:?}",
            observed.unknown
        );
        assert_eq!(
            observed.unknown.len(),
            MAX_CONCURRENT_SESSION_PROBES + 1,
            "every candidate must be classified; none may vanish as false absence"
        );
    }

    #[test]
    fn discovery_error_is_preserved_by_observe_sessions_in() {
        // NotFound is empty success; a non-directory path must surface an error rather than
        // a successful empty inventory.
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("not-a-dir");
        fs::write(&file_path, b"x").unwrap();
        let err = observe_sessions_in(&file_path).unwrap_err();
        assert_ne!(err, io::ErrorKind::NotFound);
    }

    #[test]
    fn unknown_sessions_with_layouts_are_not_marked_exited() {
        let observation = SessionObservation {
            responsive: vec![("live".to_owned(), Duration::from_secs(1))],
            unknown: vec![("slow".to_owned(), Duration::from_secs(2))],
        };
        let resurrectable = vec![
            ("slow".to_owned(), Duration::from_secs(9)),
            ("dead".to_owned(), Duration::from_secs(8)),
        ];
        let displayed = sessions_for_display(&observation, resurrectable);
        let by_name: HashMap<_, _> = displayed
            .into_iter()
            .map(|(name, ts, is_dead)| (name, (ts, is_dead)))
            .collect();
        assert_eq!(by_name.get("live"), Some(&(Duration::from_secs(1), false)));
        assert_eq!(by_name.get("dead"), Some(&(Duration::from_secs(8), true)));
        assert!(
            !by_name.contains_key("slow"),
            "unresponsive+layout must not appear as EXITED: {:?}",
            by_name
        );
    }

    #[cfg(unix)]
    #[test]
    fn observational_probe_does_not_unlink_on_connection_refused() {
        use std::os::unix::net::UnixListener;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stale-session");
        // std's UnixListener leaves the pathname on drop; interprocess unlinks.
        {
            let _listener = UnixListener::bind(&path).unwrap();
        }
        assert!(
            path_try_exists(&path).unwrap(),
            "dropping the listener must leave the pathname"
        );
        assert_eq!(
            assert_socket_path(&path, Duration::from_secs(1)),
            SessionProbeResult::Absent
        );
        assert!(
            path_try_exists(&path).unwrap(),
            "ordinary probing must not reclaim a refused socket without a claim"
        );
    }

    #[cfg(unix)]
    #[test]
    fn reclaim_under_claim_unlinks_after_identity_recheck() {
        use crate::session_socket_claim::SessionSocketClaim;
        use std::os::unix::net::UnixListener;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reclaim-session");
        {
            let _listener = UnixListener::bind(&path).unwrap();
        }
        let claim = SessionSocketClaim::try_acquire(&path).unwrap();
        claim.reclaim_stale_socket(&path).unwrap();
        assert!(
            !path_try_exists(&path).unwrap(),
            "owner may reclaim after confirming the path is still abandoned"
        );
        claim.release();
    }

    #[cfg(unix)]
    #[test]
    fn reclaim_under_claim_leaves_a_live_replacement_alone() {
        use crate::session_socket_claim::SessionSocketClaim;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("live-session");
        // Listener without the claim models a path that became live again before reclaim.
        let _listener = crate::consts::ipc_bind(&path).unwrap();
        let claim = SessionSocketClaim::try_acquire(&path).unwrap();
        let err = claim.reclaim_stale_socket(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AddrInUse);
        assert!(
            path_try_exists(&path).unwrap(),
            "a live socket must survive reclaim"
        );
        claim.release();
    }

    #[cfg(unix)]
    #[test]
    fn exact_target_probe_classifies_missing_socket_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing");
        assert_eq!(
            assert_socket_path(&path, Duration::from_millis(200)),
            SessionProbeResult::Absent
        );
    }

    #[cfg(unix)]
    #[test]
    fn timed_connect_to_a_missing_path_returns_promptly() {
        use crate::consts::ipc_connect_with_timeout;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing-socket");
        let started = Instant::now();
        let err = ipc_connect_with_timeout(&path, Duration::from_millis(200)).unwrap_err();
        let elapsed = started.elapsed();
        assert!(
            matches!(
                err.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ),
            "unexpected error kind: {err:?}"
        );
        assert!(
            elapsed < Duration::from_millis(500),
            "timed connect to a missing path took {elapsed:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn probe_deadline_covers_connect_send_and_receive_against_a_live_server() {
        use interprocess::local_socket::traits::Listener as _;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("live-probe");
        let listener = crate::consts::ipc_bind(&path).unwrap();
        let server = thread::spawn(move || {
            let stream = listener.accept().expect("accept probe client");
            let mut receiver: IpcReceiverWithContext<ClientToServerMsg> =
                IpcReceiverWithContext::new(stream);
            let (msg, _) = receiver.recv_client_msg().expect("ConnStatus");
            assert!(matches!(msg, ClientToServerMsg::ConnStatus));
            let mut sender: IpcSenderWithContext<ServerToClientMsg> =
                receiver.get_sender().expect("clone probe reply sender");
            sender
                .send_server_msg(ServerToClientMsg::Connected)
                .expect("Connected reply");
        });
        let started = Instant::now();
        assert_eq!(
            assert_socket_path(&path, Duration::from_secs(2)),
            SessionProbeResult::Responsive
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "live probe overran its deadline"
        );
        server.join().unwrap();
    }
}

#[cfg(all(test, unix))]
mod live_session_state_tests {
    use super::*;
    use interprocess::local_socket::traits::Listener as _;
    use std::os::unix::net::UnixListener as StdUnixListener;
    use std::thread;

    fn answer_one_probe(path: PathBuf) -> thread::JoinHandle<()> {
        let listener = crate::consts::ipc_bind(&path).unwrap();
        thread::spawn(move || {
            let stream = listener.accept().expect("accept session probe");
            let mut receiver: IpcReceiverWithContext<ClientToServerMsg> =
                IpcReceiverWithContext::new(stream);
            let (msg, _) = receiver.recv_client_msg().expect("session probe");
            assert!(matches!(msg, ClientToServerMsg::ConnStatus));
            let mut sender: IpcSenderWithContext<ServerToClientMsg> =
                receiver.get_sender().expect("probe reply sender");
            sender
                .send_server_msg(ServerToClientMsg::Connected)
                .expect("probe reply");
        })
    }

    fn write_metadata(cache_dir: &Path, id: &str, info: &SessionInfo) {
        let folder = cache_dir.join(id);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("session-metadata.kdl"), info.to_string()).unwrap();
    }

    #[test]
    fn a_stale_socket_path_is_not_reported_live() {
        let sock_dir = tempfile::tempdir().unwrap();
        let cache_dir = tempfile::tempdir().unwrap();
        let socket = sock_dir.path().join("crashed-session");
        {
            let _listener = StdUnixListener::bind(&socket).unwrap();
        }
        assert!(socket.exists(), "std listener leaves its pathname behind");
        write_metadata(
            cache_dir.path(),
            "crashed-session",
            &SessionInfo::new("crashed-session".to_owned()),
        );

        let states = read_live_session_states_with_timeout(
            "",
            sock_dir.path(),
            cache_dir.path(),
            Duration::from_millis(200),
        );
        assert!(states.is_empty());
    }

    #[test]
    fn an_unresponsive_socket_stays_unknown_and_is_synthesized_without_metadata() {
        let sock_dir = tempfile::tempdir().unwrap();
        let cache_dir = tempfile::tempdir().unwrap();
        let _listener = StdUnixListener::bind(sock_dir.path().join("wedged")).unwrap();

        let states = read_live_session_states_with_timeout(
            "current",
            sock_dir.path(),
            cache_dir.path(),
            Duration::from_millis(50),
        );
        let info = states
            .get("wedged")
            .expect("unknown sessions must not be treated as absent");
        assert_eq!(info.name, "wedged");
        assert!(!info.is_current_session);
    }

    #[test]
    fn a_responsive_session_without_metadata_is_still_listed() {
        let sock_dir = tempfile::tempdir().unwrap();
        let cache_dir = tempfile::tempdir().unwrap();
        let socket = sock_dir.path().join("fresh");
        let server = answer_one_probe(socket);

        let states = read_live_session_states_with_timeout(
            "fresh",
            sock_dir.path(),
            cache_dir.path(),
            Duration::from_secs(2),
        );
        let info = states.get("fresh").expect("responsive session listed");
        assert_eq!(info.name, "fresh");
        assert!(info.is_current_session);
        assert!(info.tabs.is_empty());
        server.join().unwrap();
    }

    #[test]
    fn non_force_delete_treats_unresponsive_sessions_as_active() {
        let observation = SessionObservation {
            responsive: vec![("live".to_owned(), Duration::from_secs(1))],
            unknown: vec![("wedged".to_owned(), Duration::from_secs(2))],
        };
        let resurrectable = vec![
            ("live".to_owned(), Duration::from_secs(1)),
            ("wedged".to_owned(), Duration::from_secs(2)),
            ("dead".to_owned(), Duration::from_secs(3)),
        ];

        assert_eq!(
            resurrectable_sessions_to_delete(false, &observation, resurrectable),
            vec![("dead".to_owned(), Duration::from_secs(3))]
        );
    }

    #[test]
    fn force_delete_includes_unresponsive_sessions() {
        let observation = SessionObservation {
            responsive: vec![],
            unknown: vec![("wedged".to_owned(), Duration::from_secs(2))],
        };
        let resurrectable = vec![("wedged".to_owned(), Duration::from_secs(2))];

        assert_eq!(
            resurrectable_sessions_to_delete(true, &observation, resurrectable.clone()),
            resurrectable
        );
    }
}

#[cfg(test)]
mod session_name_match_tests {
    use super::*;

    #[test]
    fn match_session_name_exact_probe_is_not_none_when_the_responsive_list_omits_it() {
        // Direct probe Responsive or Unknown, responsive listing omits the name.
        let responsive = vec![("other".to_owned(), Duration::from_secs(1))];
        let unknown = vec![];
        let exact = decide_session_name_match(
            "kept",
            &responsive,
            &unknown,
            SessionProbeResult::Responsive,
        )
        .unwrap();
        assert_eq!(exact, SessionNameMatch::Exact("kept".to_owned()));
        assert_ne!(exact, SessionNameMatch::None);

        let timed_out =
            decide_session_name_match("kept", &responsive, &unknown, SessionProbeResult::Unknown);
        assert_eq!(timed_out.unwrap_err(), io::ErrorKind::TimedOut);

        let missing =
            decide_session_name_match("kept", &responsive, &unknown, SessionProbeResult::Absent)
                .unwrap();
        assert_eq!(missing, SessionNameMatch::None);
    }

    #[test]
    fn match_session_name_unknown_candidate_is_not_reported_missing() {
        let timed_out = decide_session_name_match(
            "kept",
            &[],
            &[("kept".to_owned(), Duration::from_secs(2))],
            SessionProbeResult::Absent,
        );
        assert_eq!(timed_out.unwrap_err(), io::ErrorKind::TimedOut);

        let ambiguous = decide_session_name_match(
            "pre",
            &[("present".to_owned(), Duration::from_secs(1))],
            &[("prefix-unknown".to_owned(), Duration::from_secs(2))],
            SessionProbeResult::Absent,
        )
        .unwrap();
        match ambiguous {
            SessionNameMatch::AmbiguousPrefix(names) => {
                assert!(names.iter().any(|name| name == "present"));
                assert!(names.iter().any(|name| name == "prefix-unknown"));
            },
            other => panic!("unknown prefix candidate must stay visible, got {other:?}"),
        }
    }

    #[test]
    fn session_name_mtime_error_is_not_absence() {
        let error = session_mtime_result(Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "metadata",
        )));
        assert!(
            error.is_err(),
            "metadata failure must not look like no session"
        );
    }

    #[test]
    fn resurrectable_layout_presence_error_is_not_absence() {
        assert_eq!(
            layout_presence(Err(io::ErrorKind::PermissionDenied)),
            LayoutPresence::Unknown
        );
        assert_ne!(
            layout_presence(Err(io::ErrorKind::PermissionDenied)),
            LayoutPresence::Absent
        );
        assert!(layout_counts_as_resurrectable(layout_presence(Err(
            io::ErrorKind::PermissionDenied
        ))));
        assert!(!layout_counts_as_resurrectable(layout_presence(Ok(false))));
        assert_eq!(layout_presence(Ok(false)), LayoutPresence::Absent);
        assert_eq!(layout_presence(Ok(true)), LayoutPresence::Present);
    }
}

#[cfg(test)]
mod windows_pid_liveness_tests {
    use super::*;

    #[test]
    fn access_denied_stays_unknown() {
        assert_eq!(
            classify_windows_open_process(true, WINDOWS_ERROR_ACCESS_DENIED),
            WindowsPidLiveness::Unknown
        );
    }

    #[test]
    fn invalid_parameter_is_dead() {
        assert_eq!(
            classify_windows_open_process(true, WINDOWS_ERROR_INVALID_PARAMETER),
            WindowsPidLiveness::Dead
        );
    }

    #[test]
    fn successful_handle_is_alive_but_unverified() {
        assert_eq!(
            classify_windows_open_process(false, 0),
            WindowsPidLiveness::AliveUnverified
        );
    }

    #[test]
    fn windows_liveness_alive_unverified_is_not_responsive() {
        assert_eq!(
            probe_result_for_windows_liveness(WindowsPidLiveness::AliveUnverified),
            SessionProbeResult::Unknown
        );
        assert_eq!(
            probe_result_for_windows_liveness(WindowsPidLiveness::Dead),
            SessionProbeResult::Absent
        );
        assert_eq!(
            probe_result_for_windows_liveness(WindowsPidLiveness::Unknown),
            SessionProbeResult::Unknown
        );
        assert_eq!(
            probe_result_for_windows_liveness(classify_windows_open_process(
                true,
                WINDOWS_ERROR_ACCESS_DENIED,
            )),
            SessionProbeResult::Unknown
        );
        assert_ne!(
            probe_result_for_windows_liveness(WindowsPidLiveness::AliveUnverified),
            SessionProbeResult::Responsive
        );
    }

    #[test]
    fn null_handle_without_a_known_death_error_is_unknown() {
        assert_eq!(
            classify_windows_open_process(true, 0),
            WindowsPidLiveness::Unknown
        );
        assert_eq!(
            classify_windows_open_process(true, 12345),
            WindowsPidLiveness::Unknown
        );
    }
}

#[cfg(all(test, unix))]
mod concurrent_creation_claim_tests {
    use super::*;
    use crate::session_socket_claim::SessionSocketClaim;
    use std::sync::{Arc, Barrier};
    use std::thread;

    #[test]
    fn only_one_of_two_concurrent_creators_wins_the_socket_claim() {
        let dir = tempfile::tempdir().unwrap();
        let path = Arc::new(dir.path().join("race-session"));
        let barrier = Arc::new(Barrier::new(2));
        let wins = Arc::new(AtomicUsize::new(0));
        let losses = Arc::new(AtomicUsize::new(0));

        std::thread::scope(|scope| {
            for _ in 0..2 {
                let path = Arc::clone(&path);
                let barrier = Arc::clone(&barrier);
                let wins = Arc::clone(&wins);
                let losses = Arc::clone(&losses);
                scope.spawn(move || {
                    barrier.wait();
                    match SessionSocketClaim::try_acquire(&path) {
                        Ok(claim) => {
                            wins.fetch_add(1, Ordering::SeqCst);
                            thread::sleep(Duration::from_millis(50));
                            claim.release();
                        },
                        Err(err) if err.kind() == io::ErrorKind::AddrInUse => {
                            losses.fetch_add(1, Ordering::SeqCst);
                        },
                        Err(err) => panic!("unexpected claim error: {err}"),
                    }
                });
            }
        });

        assert_eq!(wins.load(Ordering::SeqCst), 1);
        assert_eq!(losses.load(Ordering::SeqCst), 1);
    }
}

#[cfg(test)]
mod layout_creation_time_tests {
    use super::*;

    #[test]
    fn recorded_creation_time_survives_replacing_the_layout_file() {
        let dir = tempfile::tempdir().unwrap();
        let layout = dir.path().join("session-layout.kdl");
        fs::write(&layout, "layout { }").unwrap();
        // pretend the session was created an hour ago
        let an_hour_ago = SystemTime::now() - Duration::from_secs(3600);
        let seconds = an_hour_ago
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        fs::write(session_layout_created_marker(&layout), seconds.to_string()).unwrap();

        // replace the layout file the way the server does: temp file + rename
        let replacement = dir.path().join(".tmp-layout");
        fs::write(&replacement, "layout { pane }").unwrap();
        fs::rename(&replacement, &layout).unwrap();
        record_session_layout_creation_time(&layout);

        let age = session_layout_creation_time(&layout)
            .unwrap()
            .elapsed()
            .unwrap();
        assert!(age >= Duration::from_secs(3590), "{:?}", age);
    }

    #[test]
    fn first_recording_keeps_an_existing_layout_files_time_and_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let layout = dir.path().join("session-layout.kdl");
        // no layout yet: a missing layout is an error, not a time
        assert_eq!(
            session_layout_creation_time(&layout).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        record_session_layout_creation_time(&layout);
        fs::write(&layout, "layout { }").unwrap();
        let first = session_layout_creation_time(&layout).unwrap();
        std::thread::sleep(Duration::from_millis(1100));
        record_session_layout_creation_time(&layout);
        assert_eq!(session_layout_creation_time(&layout).unwrap(), first);
    }

    #[test]
    fn an_out_of_range_recorded_time_falls_back_to_file_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let layout = dir.path().join("session-layout.kdl");
        fs::write(&layout, "layout { }").unwrap();
        fs::write(session_layout_created_marker(&layout), u64::MAX.to_string()).unwrap();
        let metadata = fs::metadata(&layout).unwrap();
        let fallback = metadata
            .created()
            .or_else(|_| metadata.modified())
            .expect("the fixture file has a timestamp");
        assert_eq!(session_layout_creation_time(&layout).unwrap(), fallback);
    }
}

#[cfg(test)]
mod verified_layout_read_tests {
    use super::*;
    use crate::session_serialization::pane_contents_file_name;
    use tempfile::tempdir;

    fn layout_referencing(file_name: &str) -> String {
        format!(
            r#"
layout {{
    pane contents_file="{}"
}}
"#,
            file_name
        )
    }

    fn loaded_contents(layout: &Layout) -> Vec<String> {
        fn tiled(pane: &crate::input::layout::TiledPaneLayout, contents: &mut Vec<String>) {
            if let Some(pane_contents) = pane.pane_initial_contents.as_ref() {
                contents.push(pane_contents.clone());
            }
            for child in &pane.children {
                tiled(child, contents);
            }
        }
        fn floating(
            panes: &[crate::input::layout::FloatingPaneLayout],
            contents: &mut Vec<String>,
        ) {
            for pane in panes {
                if let Some(pane_contents) = pane.pane_initial_contents.as_ref() {
                    contents.push(pane_contents.clone());
                }
            }
        }
        let mut contents = vec![];
        for (_, tiled_pane, floating_panes) in &layout.tabs {
            tiled(tiled_pane, &mut contents);
            floating(floating_panes, &mut contents);
        }
        if let Some((tiled_pane, floating_panes)) = layout.template.as_ref() {
            tiled(tiled_pane, &mut contents);
            floating(floating_panes, &mut contents);
        }
        contents.sort();
        contents
    }

    #[test]
    fn a_stable_resurrection_layout_is_read_once() {
        let dir = tempdir().unwrap();
        let layout_path = dir.path().join("session-layout.kdl");
        let contents_file = pane_contents_file_name("stable contents");
        fs::write(dir.path().join(&contents_file), "stable contents").unwrap();
        fs::write(&layout_path, layout_referencing(&contents_file)).unwrap();

        let mut reads = 0;
        let layout = resurrection_layout_from_path_with_reader_barrier(&layout_path, || {
            reads += 1;
        })
        .unwrap()
        .unwrap();

        assert_eq!(reads, 1);
        assert_eq!(loaded_contents(&layout), vec!["stable contents".to_owned()]);
    }

    #[test]
    fn resurrection_restarts_when_content_files_are_replaced_between_attempts() {
        let dir = tempdir().unwrap();
        let layout_path = dir.path().join("session-layout.kdl");
        let replacement_path = dir.path().join("replacement-layout.kdl");
        let old_contents_file = pane_contents_file_name("old contents");
        let new_contents_file = pane_contents_file_name("new contents");
        fs::write(dir.path().join(&old_contents_file), "old contents").unwrap();
        fs::write(dir.path().join(&new_contents_file), "new contents").unwrap();
        fs::write(&layout_path, layout_referencing(&old_contents_file)).unwrap();
        fs::write(&replacement_path, layout_referencing(&new_contents_file)).unwrap();

        let mut reads = 0;
        let layout = resurrection_layout_from_path_with_reader_barrier(&layout_path, || {
            reads += 1;
            if reads == 1 {
                fs::remove_file(dir.path().join(&old_contents_file)).unwrap();
                fs::rename(&replacement_path, &layout_path).unwrap();
            }
        })
        .unwrap()
        .unwrap();

        assert_eq!(reads, 2);
        assert_eq!(loaded_contents(&layout), vec!["new contents".to_owned()]);
    }
}

#[cfg(all(test, unix))]
#[path = "probe_deadline.rs"]
mod probe_deadline;

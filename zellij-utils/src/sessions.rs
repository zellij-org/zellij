use crate::{
    consts::{
        is_ipc_socket, session_info_folder_for_session, session_layout_cache_file_name,
        ZELLIJ_SESSION_INFO_CACHE_DIR, ZELLIJ_SOCK_DIR,
    },
    data::SessionInfo,
    envs,
    input::layout::Layout,
    ipc::{ClientToServerMsg, IpcReceiverWithContext, IpcSenderWithContext, ServerToClientMsg},
    session_index::{self, SessionIndex},
};
use anyhow;
use humantime::format_duration;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
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
pub fn rename_session(session_id: &str, new_name: &str) {
    if let Err(e) = SessionIndex::open_default()
        .and_then(|index| index.rename_by_id(session_id, new_name, now_secs()))
    {
        log::error!(
            "Failed to record the new name of session {:?} in the session index: {}",
            session_id,
            e
        );
    }
}

/// The id of the session with this name, if the session index knows it. Callers fall back to the
/// name itself, which is the id of sessions from before ids.
pub fn resolve_session_id(name: &str) -> Option<String> {
    session_index::session_id_for_name(name)
}

/// The socket path of the session with this name, if the session index knows it.
pub fn resolve_session_socket_path(name: &str) -> Option<PathBuf> {
    resolve_session_id(name).map(|id| ZELLIJ_SOCK_DIR.join(id))
}

/// The name of the session with this id (a socket or cache folder name), or the id itself for
/// sessions from before ids.
pub fn display_name_for_session_id(id: &str) -> String {
    session_index::session_name_for_id(id).unwrap_or_else(|| id.to_owned())
}

fn session_name(names_by_id: &HashMap<String, String>, id: String) -> String {
    names_by_id.get(&id).cloned().unwrap_or(id)
}

/// How long the socket of a single session may take to answer before that
/// session is considered dead, and how long listing may take in total.
const SESSION_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
/// Upper bound on sockets probed at once, so a directory full of wedged
/// sockets is scanned in parallel without one thread per entry.
const MAX_CONCURRENT_SESSION_PROBES: usize = 8;

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

/// Probe `(id, name, payload)` candidates concurrently and return the live ones
/// in candidate order. A single overall deadline bounds the whole call: when it
/// expires, sockets that have not been probed yet are treated as dead.
fn live_sessions<T: Clone + Send + Sync>(
    dir: &Path,
    candidates: Vec<(String, String, T)>,
    deadline: Duration,
) -> Vec<(String, T)> {
    live_sessions_with(candidates, deadline, |id, remaining| {
        assert_socket_in(dir, id, remaining)
    })
}

fn live_sessions_with<T, F>(
    candidates: Vec<(String, String, T)>,
    deadline: Duration,
    probe: F,
) -> Vec<(String, T)>
where
    T: Clone + Send + Sync,
    F: Fn(&str, Duration) -> bool + Sync,
{
    if candidates.is_empty() {
        return Vec::new();
    }
    if candidates.len() == 1 {
        let (id, name, payload) = candidates.into_iter().next().unwrap();
        return if probe(&id, deadline) {
            vec![(name, payload)]
        } else {
            Vec::new()
        };
    }
    let results: Vec<Mutex<Option<(String, T)>>> =
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
                if remaining.is_zero() {
                    break;
                }
                if probe(id, remaining) {
                    let payload = candidates[index].2.clone();
                    *results[index]
                        .lock()
                        .expect("session probe result lock poisoned") =
                        Some((name.clone(), payload));
                }
            });
        }
    });
    results
        .into_iter()
        .filter_map(|slot| slot.into_inner().unwrap_or_else(|e| e.into_inner()))
        .collect()
}

pub fn get_sessions() -> Result<Vec<(String, Duration)>, io::ErrorKind> {
    get_sessions_in(&ZELLIJ_SOCK_DIR)
}

fn get_sessions_in(dir: &Path) -> Result<Vec<(String, Duration)>, io::ErrorKind> {
    get_sessions_in_with(dir, SESSION_PROBE_TIMEOUT)
}

fn get_sessions_in_with(
    dir: &Path,
    deadline: Duration,
) -> Result<Vec<(String, Duration)>, io::ErrorKind> {
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
            Ok(live_sessions(dir, candidates, deadline))
        },
        Err(err) if io::ErrorKind::NotFound != err.kind() => Err(err.kind()),
        Err(_) => Ok(Vec::with_capacity(0)),
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
                    if std::path::Path::new(&layout_file_name).exists() {
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
                    if !hidden && std::path::Path::new(&resurrection_layout_file).exists() {
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
            let candidates = files
                .flatten()
                .filter_map(|file| {
                    let id = session_socket_id(&file)?;
                    let modified_at = file.metadata().ok()?.modified().ok()?;
                    let name = session_name(&names_by_id, id.clone());
                    Some((id, name, modified_at))
                })
                .collect();
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
#[cfg(unix)]
fn assert_socket_in(dir: &Path, name: &str, timeout: Duration) -> bool {
    use crate::consts::ipc_connect;
    if timeout.is_zero() {
        return false;
    }
    let path = dir.join(name);
    match ipc_connect(&path) {
        Ok(stream) => {
            // a wedged server accepts connections but never replies; without a
            // read timeout `zellij ls` would block forever on recv below
            {
                use interprocess::local_socket::prelude::*;
                let _ = stream.set_recv_timeout(Some(timeout));
            }
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
            drop(fs::remove_file(&path));
            false
        },
        Err(_) => false,
    }
}

/// On Windows, reads the server PID from the marker file and checks whether
/// the process is still alive via `OpenProcess`. Cleans up stale marker files.
#[cfg(windows)]
fn assert_socket_in(dir: &Path, name: &str, _timeout: Duration) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    let path = dir.join(name);
    let pid_str = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => {
            drop(fs::remove_file(&path));
            return false;
        },
    };
    let pid: u32 = match pid_str.trim().parse() {
        Ok(p) => p,
        Err(_) => {
            // Marker file exists but has no valid PID (e.g. empty from old version).
            // Treat as stale.
            drop(fs::remove_file(&path));
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
        drop(fs::remove_file(&path));
    }
    alive
}

#[cfg(not(any(unix, windows)))]
fn assert_socket_in(_dir: &Path, _name: &str, _timeout: Duration) -> bool {
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
    let path = &*resolve_session_socket_path(name).unwrap_or_else(|| ZELLIJ_SOCK_DIR.join(name));
    if let Err(e) = send_kill_session(path) {
        eprintln!("Error occurred: {:?}", e);
        process::exit(1);
    }
}

/// On Windows the server answers on the connection's reply pipe, and waiting for that answer
/// keeps the request from being dropped before the server has read it.
#[cfg(windows)]
pub fn send_kill_session(path: &std::path::Path) -> std::io::Result<()> {
    let (stream, reply_stream) = crate::pipe_pairs::connect_pair(path)?;
    let _ = IpcSenderWithContext::<ClientToServerMsg>::new(stream)
        .send_client_msg(ClientToServerMsg::KillSession);
    let mut receiver: IpcReceiverWithContext<ServerToClientMsg> =
        IpcReceiverWithContext::new(reply_stream);
    let _ = receiver.recv_server_msg();
    Ok(())
}

#[cfg(not(windows))]
pub fn send_kill_session(path: &std::path::Path) -> std::io::Result<()> {
    let stream = crate::consts::ipc_connect(path)?;
    let _ = IpcSenderWithContext::<ClientToServerMsg>::new(stream)
        .send_client_msg(ClientToServerMsg::KillSession);
    Ok(())
}

pub fn delete_session(name: &str, force: bool) {
    // The socket and the cache folder are named by the session id; the index row by the name.
    let session_id = resolve_session_id(name).unwrap_or_else(|| name.to_owned());
    if let Err(e) = validate_session_folder_name(&session_id) {
        eprintln!("{}", e);
        process::exit(1);
    }
    if force {
        let _ = send_kill_session(&ZELLIJ_SOCK_DIR.join(&session_id));
    }
    if let Ok(index) = crate::session_index::SessionIndex::open_default() {
        let _ = index.delete(name);
    }
    if let Err(e) = std::fs::remove_dir_all(session_info_folder_for_session(&session_id)) {
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
        Ok(_) => {
            // A socket that could not be probed before the shared deadline is
            // unknown, not absent. Treating it as absent lets callers create a
            // second server for a live session.
            let socket_path =
                resolve_session_socket_path(name).unwrap_or_else(|| ZELLIJ_SOCK_DIR.join(name));
            if socket_path.exists() {
                Err(io::ErrorKind::TimedOut)
            } else {
                Ok(false)
            }
        },
        Err(e) => Err(e),
    }
}

// if the session is resurrecable, the returned layout is the one to be used to resurrect it
pub fn resurrection_layout(session_name_to_resurrect: &str) -> Result<Option<Layout>, String> {
    let session_id = resolve_session_id(session_name_to_resurrect)
        .unwrap_or_else(|| session_name_to_resurrect.to_owned());
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
    let mut other_session_names: Vec<(String, Duration)> = vec![];
    let mut session_infos_on_machine = BTreeMap::new();
    if let Ok(files) = fs::read_dir(sock_dir) {
        files.for_each(|file| {
            if let Ok(file) = file {
                if let Ok(file_name) = file.file_name().into_string() {
                    if !crate::consts::is_app_socket_name(&file_name)
                        && file
                            .file_type()
                            .map(|file_type| is_ipc_socket(&file_type))
                            .unwrap_or(false)
                    {
                        let creation_time = std::fs::metadata(&file.path())
                            .ok()
                            .and_then(|f| f.created().ok().or_else(|| f.modified().ok()))
                            .and_then(|d| d.elapsed().ok())
                            .unwrap_or_default();
                        other_session_names.push((file_name, creation_time));
                    }
                }
            }
        });
    }

    let names_by_id = session_index::session_names_by_id();
    for (session_id, creation_time) in other_session_names {
        let session_cache_file_name = session_info_cache_dir
            .join(&session_id)
            .join("session-metadata.kdl");
        if let Ok(raw_session_info) = fs::read_to_string(&session_cache_file_name) {
            if let Ok(mut session_info) =
                SessionInfo::from_string(&raw_session_info, current_session_name)
            {
                session_info.creation_time = creation_time;
                session_infos_on_machine
                    .insert(session_name(&names_by_id, session_id), session_info);
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
mod session_entry_tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn non_utf8_and_non_socket_directory_entries_are_skipped() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join(OsString::from_vec(vec![b'n', 0xff])),
            b"",
        )
        .unwrap();
        fs::write(directory.path().join("regular-file"), b"").unwrap();

        for entry in fs::read_dir(directory.path()).unwrap() {
            let entry = entry.unwrap();
            assert!(session_socket_id(&entry).is_none());
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

        let live = live_sessions_with(candidates, Duration::from_secs(2), |id, _| {
            if id == "slow" {
                thread::sleep(Duration::from_millis(300));
            }
            true
        });

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
        let sessions =
            live_sessions_with(candidates, Duration::from_millis(500), |id, remaining| {
                if id == "wedged" {
                    thread::sleep(remaining);
                    false
                } else {
                    true
                }
            });

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].0, "live");
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
        let sessions =
            live_sessions_with(candidates, Duration::from_millis(400), |_, remaining| {
                // A wedged socket spends its whole budget waiting for a reply.
                thread::sleep(remaining);
                false
            });
        let elapsed = started.elapsed();

        assert!(sessions.is_empty());
        // Sequentially this would take twelve probe timeouts.
        assert!(
            elapsed < Duration::from_secs(2),
            "probing twelve wedged sockets took {:?}",
            elapsed
        );
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

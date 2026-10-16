use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use interprocess::local_socket::prelude::*;
use zellij_utils::attention::{GlobalAttention, SessionAttention};
use zellij_utils::ipc::{ClientToServerMsg, IpcReceiverWithContext, ServerToClientMsg};
use zellij_utils::session_link::{request_session_attention_with, PEER_REPLY_TIMEOUT};

pub const FOLDER_POLL: Duration = Duration::from_millis(500);
pub const FULL_PULL: Duration = Duration::from_secs(5);
const WORKERS: usize = 4;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub global: GlobalAttention,
    pub last_used: HashMap<String, u64>,
}

#[derive(Debug)]
pub enum Job {
    Pushed {
        session: String,
        attention: Option<SessionAttention>,
    },
    Refresh,
    Exit,
}

pub struct Watcher {
    jobs: Sender<Job>,
    details: Arc<AtomicBool>,
    live: Arc<Mutex<BTreeSet<String>>>,
    _listener: Option<AttentionListener>,
}

impl Watcher {
    pub fn spawn(
        sock_dir: PathBuf,
        details: bool,
        post: impl Fn(Snapshot) + Send + 'static,
    ) -> Result<Watcher> {
        let (jobs, received) = channel();
        let details = Arc::new(AtomicBool::new(details));
        let live = Arc::new(Mutex::new(BTreeSet::new()));
        let listener = match AttentionListener::bind(&sock_dir, jobs.clone()) {
            Ok(listener) => Some(listener),
            Err(e) => {
                report!(
                    "sessions cannot push changes to the app, so it falls back to polling: {:#}",
                    e
                );
                None
            },
        };
        let state = WatchState {
            sock_dir,
            sessions: BTreeMap::new(),
            listed: BTreeSet::new(),
            details: details.clone(),
            live: live.clone(),
            last_posted: None,
        };
        std::thread::Builder::new()
            .name("app_sessions".to_owned())
            .spawn(move || state.run(received, post))?;
        Ok(Watcher {
            jobs,
            details,
            live,
            _listener: listener,
        })
    }

    pub fn set_details(&self, details: bool) {
        if self.details.swap(details, Ordering::Relaxed) != details {
            let _ = self.jobs.send(Job::Refresh);
        }
    }

    pub fn live_sessions(&self) -> Vec<String> {
        self.live
            .lock()
            .map(|live| live.iter().cloned().collect())
            .unwrap_or_default()
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        let _ = self.jobs.send(Job::Exit);
    }
}

struct WatchState {
    sock_dir: PathBuf,
    sessions: BTreeMap<String, SessionAttention>,
    listed: BTreeSet<String>,
    details: Arc<AtomicBool>,
    live: Arc<Mutex<BTreeSet<String>>>,
    last_posted: Option<Snapshot>,
}

impl WatchState {
    fn run(mut self, jobs: Receiver<Job>, post: impl Fn(Snapshot)) {
        let mut next_pull = Instant::now();
        let mut next_poll = Instant::now();
        loop {
            let now = Instant::now();
            if now >= next_pull {
                self.listed = self.listing();
                let names: Vec<String> = self.listed.iter().cloned().collect();
                self.sessions = pull(&names, self.details.load(Ordering::Relaxed));
                next_pull = Instant::now() + FULL_PULL;
                next_poll = Instant::now() + FOLDER_POLL;
                self.publish(&post);
            } else if now >= next_poll {
                let listed = self.listing();
                if listed != self.listed {
                    let added: Vec<String> = listed.difference(&self.listed).cloned().collect();
                    let removed: Vec<String> = self.listed.difference(&listed).cloned().collect();
                    for name in removed {
                        self.sessions.remove(&name);
                    }
                    let details = self.details.load(Ordering::Relaxed);
                    self.sessions.extend(pull(&added, details));
                    self.listed = listed;
                    self.publish(&post);
                }
                next_poll = Instant::now() + FOLDER_POLL;
            }
            let wait = next_pull
                .min(next_poll)
                .saturating_duration_since(Instant::now());
            match jobs.recv_timeout(wait) {
                Ok(Job::Exit) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(Job::Pushed { session, .. })
                    if zellij_utils::consts::is_app_socket_name(&session) => {},
                Ok(Job::Pushed { session, attention }) => {
                    match attention {
                        Some(attention) => {
                            self.sessions.insert(session, attention);
                        },
                        None => {
                            self.sessions.remove(&session);
                        },
                    }
                    self.publish(&post);
                },
                Ok(Job::Refresh) => next_pull = Instant::now(),
                Err(RecvTimeoutError::Timeout) => {},
            }
        }
    }

    fn listing(&self) -> BTreeSet<String> {
        zellij_utils::session_index::running_session_names_in(&self.sock_dir)
            .into_iter()
            .collect()
    }

    fn publish(&mut self, post: &impl Fn(Snapshot)) {
        let details = self.details.load(Ordering::Relaxed);
        let snapshot = snapshot_of(&self.sessions, details);
        if let Ok(mut live) = self.live.lock() {
            *live = self.sessions.keys().cloned().collect();
        }
        if self.last_posted.as_ref() == Some(&snapshot) {
            return;
        }
        self.last_posted = Some(snapshot.clone());
        post(snapshot);
    }
}

pub fn snapshot_of(sessions: &BTreeMap<String, SessionAttention>, details: bool) -> Snapshot {
    let global = GlobalAttention::build(
        "",
        sessions
            .iter()
            .map(|(name, attention)| (name.clone(), attention.clone()))
            .collect(),
    );
    let global = if details {
        global
    } else {
        global.without_command_text()
    };
    let last_used = sessions
        .keys()
        .filter_map(|name| last_used_ms(name).map(|ms| (name.clone(), ms)))
        .collect();
    Snapshot { global, last_used }
}

fn last_used_ms(name: &str) -> Option<u64> {
    let metadata = zellij_utils::consts::ZELLIJ_SESSION_INFO_CACHE_DIR
        .join(name)
        .join("session-metadata.kdl");
    let modified = std::fs::metadata(metadata)
        .and_then(|m| m.modified())
        .or_else(|_| {
            std::fs::metadata(zellij_utils::consts::ZELLIJ_SOCK_DIR.join(name))
                .and_then(|m| m.modified())
        })
        .ok()?;
    epoch_ms(modified)
}

fn epoch_ms(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

pub fn pull(names: &[String], details: bool) -> BTreeMap<String, SessionAttention> {
    let queue = Mutex::new(names.iter().cloned());
    let answers = Mutex::new(BTreeMap::new());
    let workers = WORKERS.min(names.len());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let Some(name) = queue.lock().ok().and_then(|mut q| q.next()) else {
                    break;
                };
                if let Ok(attention) =
                    request_session_attention_with(&name, details, PEER_REPLY_TIMEOUT)
                {
                    if let Ok(mut answers) = answers.lock() {
                        answers.insert(name, attention);
                    }
                }
            });
        }
    });
    answers.into_inner().unwrap_or_default()
}

pub struct AttentionListener {
    path: PathBuf,
}

impl AttentionListener {
    pub fn bind(sock_dir: &Path, jobs: Sender<Job>) -> Result<AttentionListener> {
        let app_dir = zellij_utils::consts::app_dir_in(sock_dir);
        zellij_utils::consts::prepare_private_dir(&app_dir)
            .with_context(|| format!("failed to create {}", app_dir.display()))?;
        let path = app_dir.join(zellij_utils::consts::APP_ATTENTION_SOCKET_NAME);
        #[cfg(unix)]
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
        let listener = zellij_utils::consts::ipc_bind(&path)
            .with_context(|| format!("failed to listen on {}", path.display()))?;
        #[cfg(windows)]
        let reply_listener = zellij_utils::consts::ipc_bind_reply(&path)
            .with_context(|| format!("failed to listen on {}-reply", path.display()))?;
        std::thread::Builder::new()
            .name("app_attention".to_owned())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else {
                        continue;
                    };
                    #[cfg(windows)]
                    let reply = match reply_listener.accept() {
                        Ok(reply) => Some(reply),
                        Err(_) => continue,
                    };
                    #[cfg(not(windows))]
                    let reply: Option<interprocess::local_socket::Stream> = None;
                    let jobs = jobs.clone();
                    let _ = std::thread::Builder::new()
                        .name("app_attention_peer".to_owned())
                        .spawn(move || receive_push(stream, reply, &jobs));
                }
            })?;
        Ok(AttentionListener { path })
    }
}

impl Drop for AttentionListener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn receive_push(
    stream: interprocess::local_socket::Stream,
    reply: Option<interprocess::local_socket::Stream>,
    jobs: &Sender<Job>,
) {
    let mut receiver: IpcReceiverWithContext<ClientToServerMsg> =
        IpcReceiverWithContext::new(stream);
    let _ = receiver.set_read_timeout(Some(Duration::from_secs(2)));
    let Ok((message, _)) = receiver.try_recv_client_msg() else {
        return;
    };
    let mut sender = match reply {
        Some(reply) => zellij_utils::ipc::IpcSenderWithContext::<ServerToClientMsg>::new(reply),
        None => receiver.get_sender::<ServerToClientMsg>(),
    };
    match message {
        ClientToServerMsg::PeerAttentionChanged {
            session_name,
            attention,
        } => {
            let _ = jobs.send(Job::Pushed {
                session: session_name,
                attention,
            });
            let _ = sender.send_server_msg(ServerToClientMsg::Connected);
        },
        ClientToServerMsg::ConnStatus => {
            let _ = sender.send_server_msg(ServerToClientMsg::Connected);
        },
        _ => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::fixtures::*;
    use zellij_utils::attention::{AttentionStatus, CommandText, CommandTextSource};
    use zellij_utils::session_link::push_session_attention_to;

    #[test]
    fn a_pushed_change_reaches_the_app_through_its_socket() {
        let dir = tempfile::TempDir::new().unwrap();
        let (jobs, received) = channel();
        let _listener = AttentionListener::bind(dir.path(), jobs).unwrap();
        let socket = zellij_utils::consts::app_dir_in(dir.path())
            .join(zellij_utils::consts::APP_ATTENTION_SOCKET_NAME);
        let attention = session(vec![item(1, AttentionStatus::NeedsYou, "x", 1)]);
        push_session_attention_to(&socket, "b", Some(attention.clone()), true).unwrap();
        match received.recv_timeout(Duration::from_secs(2)).unwrap() {
            Job::Pushed {
                session,
                attention: Some(pushed),
            } => {
                assert_eq!(session, "b");
                assert_eq!(pushed, attention);
            },
            other => panic!("{:?}", other),
        }
        push_session_attention_to(&socket, "b", None, true).unwrap();
        assert!(matches!(
            received.recv_timeout(Duration::from_secs(2)).unwrap(),
            Job::Pushed {
                attention: None,
                ..
            }
        ));
    }

    #[test]
    fn without_details_command_text_never_leaves_the_watcher() {
        let mut with_command = item(1, AttentionStatus::Failed, "exited 1: cargo test", 1);
        with_command.attention.source =
            Some(zellij_utils::attention::AttentionSource::ShellCommand);
        with_command.attention.detail.command = Some(CommandText {
            text: "cargo test".to_owned(),
            source: CommandTextSource::Shell,
        });
        let sessions: BTreeMap<String, SessionAttention> =
            [("b".to_owned(), session(vec![with_command]))]
                .into_iter()
                .collect();
        let terse = snapshot_of(&sessions, false);
        let item = &terse.global.items[0].item.attention;
        assert_eq!(item.text, "exited 1");
        assert!(item.detail.command.is_none());
        let full = snapshot_of(&sessions, true);
        assert_eq!(
            full.global.items[0].item.attention.text,
            "exited 1: cargo test"
        );
    }

    #[test]
    fn a_folder_with_no_sessions_pulls_nothing() {
        assert!(pull(&[], true).is_empty());
        assert!(pull(&["zellij-window-no-such-session".to_owned()], true).is_empty());
    }
}

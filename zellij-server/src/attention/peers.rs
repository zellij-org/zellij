use crate::screen::ScreenInstruction;
use crate::session_link::{
    push_session_attention, push_session_attention_to, request_session_attention,
    PEER_REPLY_TIMEOUT,
};
use crate::session_suggestions::SessionSuggestionsJob;
use crate::thread_bus::ThreadSenders;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use zellij_utils::attention::{GlobalAttention, SessionAttention};

pub const PEER_SCAN_INTERVAL: Duration = Duration::from_secs(5);
pub const PEER_PUSH_DEBOUNCE: Duration = Duration::from_millis(150);
pub const GLOBAL_PUBLISH_DEBOUNCE: Duration = Duration::from_millis(30);
pub const SOCKET_FOLDER_SETTLE: Duration = Duration::from_millis(150);
pub const PEER_WORKERS: usize = 4;

pub fn for_each_peer<T: Send>(
    peers: Vec<String>,
    work: impl Fn(&str) -> T + Sync,
) -> Vec<(String, T)> {
    if peers.len() <= 1 {
        return peers
            .into_iter()
            .map(|peer| {
                let answer = work(&peer);
                (peer, answer)
            })
            .collect();
    }
    let queue = Mutex::new(peers.into_iter());
    let answers = Mutex::new(Vec::new());
    let workers = PEER_WORKERS.min(queue.lock().map(|q| q.len()).unwrap_or(1));
    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let Some(peer) = queue.lock().ok().and_then(|mut q| q.next()) else {
                    break;
                };
                let answer = work(&peer);
                if let Ok(mut answers) = answers.lock() {
                    answers.push((peer, answer));
                }
            });
        }
    });
    answers.into_inner().unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PeerAttentionJob {
    LocalChanged(SessionAttention),
    PeerChanged {
        session_name: String,
        attention: Option<SessionAttention>,
    },
    SocketFolderChanged,
    Exit,
}

#[derive(Debug, Clone, Default)]
pub struct LiveSessions(Arc<RwLock<Option<BTreeSet<String>>>>);

impl LiveSessions {
    pub fn known(&self) -> Option<BTreeSet<String>> {
        self.0.read().ok().and_then(|live| live.clone())
    }
    pub fn is_live(&self, name: &str) -> Option<bool> {
        self.0
            .read()
            .ok()
            .and_then(|live| live.as_ref().map(|live| live.contains(name)))
    }
    pub fn retain_live(&self, names: Vec<String>) -> Vec<String> {
        match self.known() {
            Some(live) => names.into_iter().filter(|n| live.contains(n)).collect(),
            None => names,
        }
    }
    fn set(&self, live: BTreeSet<String>) -> bool {
        match self.0.write() {
            Ok(mut current) => {
                let changed = current.as_ref() != Some(&live);
                *current = Some(live);
                changed
            },
            Err(_) => false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PeerAttentionState {
    pub own_name: String,
    pub local: SessionAttention,
    pub peers: BTreeMap<String, SessionAttention>,
    pub scanned: bool,
}

impl PeerAttentionState {
    pub fn apply_scan(&mut self, answers: Vec<(String, Option<SessionAttention>)>) {
        self.peers.clear();
        for (name, attention) in answers {
            if name == self.own_name {
                continue;
            }
            if let Some(attention) = attention {
                self.peers.insert(name, attention);
            }
        }
        self.scanned = true;
    }
    pub fn peer_changed(&mut self, name: String, attention: Option<SessionAttention>) {
        if name == self.own_name || zellij_utils::consts::is_app_socket_name(&name) {
            return;
        }
        match attention {
            Some(attention) => {
                self.peers.insert(name, attention);
            },
            None => {
                self.peers.remove(&name);
            },
        }
    }
    pub fn live(&self) -> BTreeSet<String> {
        let mut live: BTreeSet<String> = self.peers.keys().cloned().collect();
        if !self.own_name.is_empty() {
            live.insert(self.own_name.clone());
        }
        live
    }
    pub fn global(&self) -> GlobalAttention {
        let mut sessions: Vec<(String, SessionAttention)> = self
            .peers
            .iter()
            .map(|(name, attention)| (name.clone(), attention.clone()))
            .collect();
        if !self.is_app_session() {
            sessions.push((self.own_name.clone(), self.local.clone()));
        }
        GlobalAttention::build(&self.own_name, sessions)
    }
    pub fn is_app_session(&self) -> bool {
        zellij_utils::consts::is_app_socket_name(&self.own_name)
    }
}

pub fn spawn_peer_attention_service(
    senders: ThreadSenders,
    own_name: Arc<Mutex<String>>,
    live_sessions: LiveSessions,
    suggestions: Sender<SessionSuggestionsJob>,
) -> Sender<PeerAttentionJob> {
    let (sender, receiver) = channel();
    let own_sender = sender.clone();
    let _ = thread::Builder::new()
        .name("peer_attention".to_owned())
        .spawn(move || {
            let mut service = PeerAttentionService {
                senders,
                own_name,
                live_sessions,
                suggestions,
                own_sender,
                state: PeerAttentionState::default(),
                last_pushed_name: None,
                next_scan: Instant::now(),
                push_due: None,
                publish_due: None,
                last_global: None,
                last_needs_you: None,
                watcher: None,
                sock_dir: zellij_utils::consts::ZELLIJ_SOCK_DIR.clone(),
            };
            service.run(receiver);
        });
    sender
}

struct PeerAttentionService {
    senders: ThreadSenders,
    own_name: Arc<Mutex<String>>,
    live_sessions: LiveSessions,
    suggestions: Sender<SessionSuggestionsJob>,
    own_sender: Sender<PeerAttentionJob>,
    state: PeerAttentionState,
    last_pushed_name: Option<String>,
    next_scan: Instant,
    push_due: Option<Instant>,
    publish_due: Option<Instant>,
    last_global: Option<GlobalAttention>,
    last_needs_you: Option<usize>,
    watcher: Option<crate::socket_folder_watcher::SocketFolderWatcher>,
    sock_dir: PathBuf,
}

impl PeerAttentionService {
    fn run(&mut self, receiver: Receiver<PeerAttentionJob>) {
        self.ensure_watcher();
        loop {
            let wake_at = [Some(self.next_scan), self.push_due, self.publish_due]
                .into_iter()
                .flatten()
                .min()
                .unwrap_or(self.next_scan);
            let wait = wake_at.saturating_duration_since(Instant::now());
            match receiver.recv_timeout(wait) {
                Ok(PeerAttentionJob::Exit) => break,
                Ok(job) => self.handle(job),
                Err(RecvTimeoutError::Timeout) => {},
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.refresh_own_name();
            let now = Instant::now();
            if now >= self.next_scan {
                let interval = if self.scan() {
                    PEER_SCAN_INTERVAL
                } else {
                    SOCKET_FOLDER_SETTLE
                };
                self.next_scan = Instant::now() + interval;
            }
            if self.push_due.map(|at| now >= at).unwrap_or(false) {
                self.push_due = None;
                self.push_to_peers();
            }
            if self.publish_due.map(|at| now >= at).unwrap_or(false) {
                self.publish_due = None;
                self.publish();
            }
        }
        self.watcher = None;
        self.announce_exit();
    }

    fn handle(&mut self, job: PeerAttentionJob) {
        match job {
            PeerAttentionJob::LocalChanged(attention) => {
                self.state.local = attention;
                self.push_due
                    .get_or_insert_with(|| Instant::now() + PEER_PUSH_DEBOUNCE);
                self.schedule_publish();
            },
            PeerAttentionJob::PeerChanged {
                session_name,
                attention,
            } => {
                self.state.peer_changed(session_name, attention);
                self.schedule_publish();
            },
            PeerAttentionJob::SocketFolderChanged => {
                let soon = Instant::now() + SOCKET_FOLDER_SETTLE;
                if soon < self.next_scan {
                    self.next_scan = soon;
                }
            },
            PeerAttentionJob::Exit => {},
        }
    }

    fn schedule_publish(&mut self) {
        self.publish_due
            .get_or_insert_with(|| Instant::now() + GLOBAL_PUBLISH_DEBOUNCE);
    }

    fn refresh_own_name(&mut self) {
        let name = self.own_name.lock().map(|n| n.clone()).unwrap_or_default();
        if name != self.state.own_name {
            self.state.own_name = name;
            self.state.peers.remove(&self.state.own_name.clone());
            if self.last_pushed_name.is_some() {
                self.push_due = Some(Instant::now());
            }
            self.schedule_publish();
        }
    }

    fn ensure_watcher(&mut self) {
        if self.watcher.is_some() {
            return;
        }
        let sender = self.own_sender.clone();
        match crate::socket_folder_watcher::SocketFolderWatcher::start(
            self.sock_dir.clone(),
            move || {
                let _ = sender.send(PeerAttentionJob::SocketFolderChanged);
            },
        ) {
            Ok(watcher) => self.watcher = Some(watcher),
            Err(e) => log::error!("Failed to watch the session socket folder: {}", e),
        }
    }

    fn peer_names(&self) -> Vec<String> {
        let names_by_id = zellij_utils::session_index::session_names_by_id();
        zellij_utils::session_index::running_session_names_in(&self.sock_dir, &names_by_id)
            .into_iter()
            .filter(|name| name != &self.state.own_name)
            .collect()
    }

    fn scan(&mut self) -> bool {
        if self.state.own_name.is_empty() {
            return false;
        }
        let peers = self.peer_names();
        let answers = for_each_peer(peers, |peer| {
            request_session_attention(peer, PEER_REPLY_TIMEOUT).ok()
        });
        self.state.apply_scan(answers);
        self.publish();
        true
    }

    fn push_to_peers(&mut self) {
        let own_name = self.state.own_name.clone();
        if own_name.is_empty() || self.state.is_app_session() {
            return;
        }
        let peers: Vec<String> = self.state.peers.keys().cloned().collect();
        let previous_name = self
            .last_pushed_name
            .replace(own_name.clone())
            .filter(|previous| previous != &own_name);
        let app = self.app_socket();
        if peers.is_empty() && app.is_none() {
            return;
        }
        let attention = self.state.local.clone();
        let _ = thread::Builder::new()
            .name("peer_attention_push".to_owned())
            .spawn(move || {
                if let Some(app) = app.as_ref() {
                    if let Some(previous_name) = previous_name.as_ref() {
                        let _ = push_session_attention_to(app, previous_name, None, true);
                    }
                    let _ =
                        push_session_attention_to(app, &own_name, Some(attention.clone()), true);
                }
                for_each_peer(peers, |peer| {
                    if let Some(previous_name) = previous_name.as_ref() {
                        let _ = push_session_attention(peer, previous_name, None, true);
                    }
                    let _ = push_session_attention(peer, &own_name, Some(attention.clone()), true);
                });
            });
    }

    fn app_socket(&self) -> Option<PathBuf> {
        let path = zellij_utils::consts::app_dir_in(&self.sock_dir)
            .join(zellij_utils::consts::APP_ATTENTION_SOCKET_NAME);
        path.exists().then_some(path)
    }

    fn announce_exit(&mut self) {
        let own_name = self.state.own_name.clone();
        if own_name.is_empty() || self.state.is_app_session() {
            return;
        }
        if let Some(app) = self.app_socket() {
            let _ = push_session_attention_to(&app, &own_name, None, false);
        }
        let peers: Vec<String> = self.state.peers.keys().cloned().collect();
        for_each_peer(peers, |peer| {
            let _ = push_session_attention(peer, &own_name, None, false);
        });
    }

    fn publish(&mut self) {
        if self.state.own_name.is_empty() || !self.state.scanned {
            return;
        }
        let live_changed = self.live_sessions.set(self.state.live());
        let global = self.state.global();
        let needs_you = global.counts.needs_you;
        if self.last_global.as_ref() != Some(&global) {
            self.last_global = Some(global.clone());
            let _ = self
                .senders
                .send_to_screen(ScreenInstruction::GlobalAttentionUpdate(global));
        }
        if live_changed || self.last_needs_you != Some(needs_you) {
            self.last_needs_you = Some(needs_you);
            let _ = self
                .suggestions
                .send(SessionSuggestionsJob::GlobalAttentionChanged {
                    needs_you,
                    scanned: self.state.scanned,
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_utils::attention::{AttentionCounts, AttentionItem, AttentionStatus, PaneAttention};
    use zellij_utils::data::PaneId;

    fn attention(status: AttentionStatus) -> SessionAttention {
        let pane = PaneAttention {
            status,
            ..Default::default()
        };
        let mut counts = AttentionCounts::default();
        counts.add(&pane);
        SessionAttention {
            status: counts.highest_counted(),
            counts,
            items: vec![AttentionItem {
                pane_id: PaneId::Terminal(1),
                tab_id: 0,
                tab_name: "tab".to_owned(),
                pane_title: "pane".to_owned(),
                attention: pane,
            }],
            ..Default::default()
        }
    }

    fn state() -> PeerAttentionState {
        PeerAttentionState {
            own_name: "here".to_owned(),
            local: attention(AttentionStatus::Done),
            ..Default::default()
        }
    }

    #[test]
    fn only_sessions_that_answer_are_live() {
        let mut state = state();
        state.apply_scan(vec![
            ("api".to_owned(), Some(attention(AttentionStatus::NeedsYou))),
            ("stale".to_owned(), None),
            ("here".to_owned(), Some(attention(AttentionStatus::Failed))),
        ]);
        let live: Vec<String> = state.live().into_iter().collect();
        assert_eq!(live, vec!["api".to_owned(), "here".to_owned()]);
        let global = state.global();
        assert_eq!(global.running(), 2);
        assert_eq!(global.counts.needs_you, 1);
        assert_eq!(global.counts.done, 1);
        assert_eq!(global.counts.failed, 0);
        assert_eq!(global.top(), Some((AttentionStatus::NeedsYou, 1)));
    }

    #[test]
    fn pushed_changes_and_endings_update_the_merge() {
        let mut state = state();
        state.apply_scan(vec![]);
        state.peer_changed("api".to_owned(), Some(attention(AttentionStatus::Failed)));
        assert_eq!(state.global().counts.failed, 1);
        assert!(state.live().contains("api"));
        state.peer_changed("api".to_owned(), None);
        assert_eq!(state.global().counts.failed, 0);
        assert!(!state.live().contains("api"));
        state.peer_changed("here".to_owned(), Some(attention(AttentionStatus::Failed)));
        assert_eq!(state.global().counts.failed, 0);
    }

    #[test]
    fn the_app_panel_session_never_counts_itself_or_other_app_sessions() {
        let mut state = PeerAttentionState {
            own_name: ".zellij-app-panel".to_owned(),
            local: attention(AttentionStatus::Done),
            ..Default::default()
        };
        state.apply_scan(vec![(
            "api".to_owned(),
            Some(attention(AttentionStatus::NeedsYou)),
        )]);
        state.peer_changed(
            ".other-app".to_owned(),
            Some(attention(AttentionStatus::Failed)),
        );
        let global = state.global();
        assert_eq!(global.running(), 1);
        assert_eq!(global.counts.done, 0);
        assert_eq!(global.counts.failed, 0);
        assert!(state.is_app_session());
    }

    #[test]
    fn a_new_scan_forgets_sessions_that_stopped_answering() {
        let mut state = state();
        state.apply_scan(vec![(
            "api".to_owned(),
            Some(attention(AttentionStatus::Done)),
        )]);
        state.apply_scan(vec![("api".to_owned(), None)]);
        assert_eq!(state.global().running(), 1);
    }

    #[test]
    fn every_peer_is_visited_once_with_few_workers() {
        let peers: Vec<String> = (0..11).map(|i| format!("s{}", i)).collect();
        let mut answers = for_each_peer(peers.clone(), |peer| peer.len());
        answers.sort();
        let mut expected: Vec<(String, usize)> =
            peers.into_iter().map(|p| (p.clone(), p.len())).collect();
        expected.sort();
        assert_eq!(answers, expected);
        assert!(for_each_peer(vec![], |_| ()).is_empty());
    }

    #[cfg(unix)]
    fn app_listener(sock_dir: &std::path::Path) -> Receiver<(String, Option<SessionAttention>)> {
        use interprocess::local_socket::prelude::*;
        use zellij_utils::ipc::{ClientToServerMsg, IpcReceiverWithContext};
        let app_dir = zellij_utils::consts::app_dir_in(sock_dir);
        zellij_utils::consts::prepare_private_dir(&app_dir).unwrap();
        let path = app_dir.join(zellij_utils::consts::APP_ATTENTION_SOCKET_NAME);
        let listener = zellij_utils::consts::ipc_bind(&path).unwrap();
        let (sender, receiver) = channel();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else {
                    continue;
                };
                let mut incoming: IpcReceiverWithContext<ClientToServerMsg> =
                    IpcReceiverWithContext::new(stream);
                if let Ok((
                    ClientToServerMsg::PeerAttentionChanged {
                        session_name,
                        attention,
                    },
                    _,
                )) = incoming.try_recv_client_msg()
                {
                    let _ = sender.send((session_name, attention));
                }
            }
        });
        receiver
    }

    #[cfg(unix)]
    fn running_service(
        sock_dir: &std::path::Path,
        name: &str,
    ) -> (Sender<PeerAttentionJob>, thread::JoinHandle<()>) {
        let (sender, receiver) = channel();
        let (suggestions, _) = channel();
        let own_sender = sender.clone();
        let own_name = Arc::new(Mutex::new(name.to_owned()));
        let sock_dir = sock_dir.to_path_buf();
        let handle = thread::spawn(move || {
            let mut service = PeerAttentionService {
                senders: ThreadSenders::default(),
                own_name,
                live_sessions: LiveSessions::default(),
                suggestions,
                own_sender,
                state: PeerAttentionState::default(),
                last_pushed_name: None,
                next_scan: Instant::now(),
                push_due: None,
                publish_due: None,
                last_global: None,
                last_needs_you: None,
                watcher: None,
                sock_dir,
            };
            service.run(receiver);
        });
        (sender, handle)
    }

    #[cfg(unix)]
    #[test]
    fn a_change_is_pushed_to_the_desktop_app_and_an_exit_says_the_session_ended() {
        let dir = tempfile::tempdir().unwrap();
        let pushed = app_listener(dir.path());
        let (jobs, service) = running_service(dir.path(), "api");
        jobs.send(PeerAttentionJob::LocalChanged(attention(
            AttentionStatus::NeedsYou,
        )))
        .unwrap();
        let (session, attention) = pushed.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(session, "api");
        assert_eq!(attention.expect("the change is sent").counts.needs_you, 1);
        jobs.send(PeerAttentionJob::Exit).unwrap();
        service.join().unwrap();
        assert_eq!(
            pushed.recv_timeout(Duration::from_secs(2)).unwrap(),
            ("api".to_owned(), None)
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_panel_session_never_pushes_to_the_desktop_app() {
        let dir = tempfile::tempdir().unwrap();
        let pushed = app_listener(dir.path());
        let (jobs, service) = running_service(dir.path(), zellij_utils::consts::APP_PANEL_SESSION);
        jobs.send(PeerAttentionJob::LocalChanged(attention(
            AttentionStatus::NeedsYou,
        )))
        .unwrap();
        thread::sleep(PEER_PUSH_DEBOUNCE * 3);
        jobs.send(PeerAttentionJob::Exit).unwrap();
        service.join().unwrap();
        assert!(pushed.recv_timeout(Duration::from_millis(300)).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn without_a_desktop_app_nothing_is_pushed_and_the_exit_is_quiet() {
        let dir = tempfile::tempdir().unwrap();
        let (jobs, service) = running_service(dir.path(), "api");
        jobs.send(PeerAttentionJob::LocalChanged(attention(
            AttentionStatus::NeedsYou,
        )))
        .unwrap();
        jobs.send(PeerAttentionJob::Exit).unwrap();
        service.join().unwrap();
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            0,
            "no socket or folder is created for an app that is not running"
        );
    }

    #[test]
    fn live_sessions_filter_names_only_once_known() {
        let live = LiveSessions::default();
        let names = vec!["a".to_owned(), "b".to_owned()];
        assert_eq!(live.retain_live(names.clone()), names);
        assert_eq!(live.is_live("a"), None);
        assert!(live.set(["a".to_owned()].into_iter().collect()));
        assert!(!live.set(["a".to_owned()].into_iter().collect()));
        assert_eq!(live.retain_live(names), vec!["a".to_owned()]);
        assert_eq!(live.is_live("b"), Some(false));
    }
}

use crate::background_jobs::{run_background_command_blocking, BackgroundCommand};
use crate::plugins::PluginInstruction;
use crate::screen::ScreenInstruction;
use crate::thread_bus::ThreadSenders;
use crate::ClientId;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};
use zellij_utils::data::{
    Event, FolderRelation, FolderSessions, SessionCounts, SessionPreviewPane, SessionPreviewTab,
    SessionSuggestion, SessionSuggestionColumn, SessionSuggestions,
};
use zellij_utils::input::session_suggestions::{
    FactTiming, SessionSuggestionsConfig, FACT_HOSTNAME,
};
use zellij_utils::session_index::{
    apply_script_order, folder_key, hostname, now_secs, parse_fact_lines, parse_git_rev_parse,
    parse_ranking_script_output, rank_candidates, Candidate, RankedCandidate, SessionContext,
    SessionIndex, SessionState,
};

pub const GIT_MIN_INTERVAL: Duration = Duration::from_secs(2);
pub const GIT_TIMEOUT: Duration = Duration::from_millis(1500);
pub const FACT_TIMEOUT: Duration = Duration::from_millis(1500);
pub const LAST_USED_MIN_INTERVAL: Duration = Duration::from_secs(60);
pub const SCRIPT_CANDIDATE_LIMIT: usize = 100;
pub const SCRIPT_HARD_TIMEOUT: Duration = Duration::from_secs(10);
pub const INDEX_POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextTrigger {
    FolderChanged,
    FocusChanged,
    PromptReturned,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SessionSuggestionsJob {
    Configure {
        config: SessionSuggestionsConfig,
        session_card: bool,
        session_indicator: bool,
    },
    SessionStarted {
        name: String,
        cwd: Option<PathBuf>,
        card_for_client: Option<ClientId>,
    },
    SessionRenamed {
        old_name: String,
        new_name: String,
    },
    SessionDeleted(String),
    FocusedPaneContext {
        folder: Option<PathBuf>,
        trigger: ContextTrigger,
    },
    ClientCountChanged(usize),
    Activity,
    CommandSubmitted,
    Subscribe {
        plugin_id: u32,
        client_id: ClientId,
    },
    Unsubscribe {
        plugin_id: u32,
    },
    GetSuggestions {
        plugin_id: u32,
        client_id: ClientId,
        limit: usize,
    },
    SocketFolderChanged,
    IndexChanged,
    Exit,
}

static LAST_ACTIVITY_REPORT_SECS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn should_report_activity() -> bool {
    use std::sync::atomic::Ordering;
    let now = now_secs();
    let last = LAST_ACTIVITY_REPORT_SECS.load(Ordering::Relaxed);
    if now.saturating_sub(last) < LAST_USED_MIN_INTERVAL.as_secs() {
        return false;
    }
    LAST_ACTIVITY_REPORT_SECS
        .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
}

pub fn spawn_session_suggestions_service(senders: ThreadSenders) -> Sender<SessionSuggestionsJob> {
    let (sender, receiver) = channel();
    let own_sender = sender.clone();
    let _ = thread::Builder::new()
        .name("session_suggestions".to_owned())
        .spawn(move || {
            let mut service = SessionSuggestionsService::new(senders, own_sender);
            service.run(receiver);
        });
    sender
}

struct SessionSuggestionsService {
    senders: ThreadSenders,
    own_sender: Sender<SessionSuggestionsJob>,
    index: Option<SessionIndex>,
    index_failed: bool,
    config: SessionSuggestionsConfig,
    session_card: bool,
    session_indicator: bool,
    session_name: Option<String>,
    context: SessionContext,
    last_git_run: Option<Instant>,
    pending_git_at: Option<Instant>,
    pending_git_folder_change: bool,
    attached_clients: usize,
    live_fact_cache: HashMap<PathBuf, BTreeMap<String, String>>,
    start_facts: Option<BTreeMap<String, String>>,
    last_used_written: Option<Instant>,
    subscribers: BTreeSet<(u32, ClientId)>,
    last_counts: Option<SessionCounts>,
    last_folder_sessions: Option<Option<FolderSessions>>,
    watcher: Option<crate::socket_folder_watcher::SocketFolderWatcher>,
    command_submitted: bool,
    last_data_version: Option<i64>,
    next_index_poll: Option<Instant>,
}

impl SessionSuggestionsService {
    fn new(senders: ThreadSenders, own_sender: Sender<SessionSuggestionsJob>) -> Self {
        SessionSuggestionsService {
            senders,
            own_sender,
            index: None,
            index_failed: false,
            config: SessionSuggestionsConfig::default(),
            session_card: true,
            session_indicator: true,
            session_name: None,
            context: SessionContext::default(),
            last_git_run: None,
            pending_git_at: None,
            pending_git_folder_change: false,
            attached_clients: 0,
            live_fact_cache: HashMap::new(),
            start_facts: None,
            last_used_written: None,
            subscribers: BTreeSet::new(),
            last_counts: None,
            last_folder_sessions: None,
            watcher: None,
            command_submitted: false,
            last_data_version: None,
            next_index_poll: None,
        }
    }

    fn run(&mut self, receiver: Receiver<SessionSuggestionsJob>) {
        loop {
            let index_poll_at = if self.subscribers.is_empty() {
                None
            } else {
                Some(
                    *self
                        .next_index_poll
                        .get_or_insert_with(|| Instant::now() + INDEX_POLL_INTERVAL),
                )
            };
            let wake_at = match (self.pending_git_at, index_poll_at) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            let job = match wake_at {
                Some(at) => {
                    let wait = at.saturating_duration_since(Instant::now());
                    match receiver.recv_timeout(wait) {
                        Ok(job) => Some(job),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                },
                None => match receiver.recv() {
                    Ok(job) => Some(job),
                    Err(_) => break,
                },
            };
            match job {
                Some(SessionSuggestionsJob::Exit) => break,
                Some(SessionSuggestionsJob::SocketFolderChanged) => {
                    while let Ok(next) = receiver.try_recv() {
                        if next != SessionSuggestionsJob::SocketFolderChanged {
                            self.handle(next);
                        }
                    }
                    self.report_counts_to_subscribers();
                },
                Some(job) => self.handle(job),
                None => {},
            }
            if let Some(at) = self.pending_git_at {
                if Instant::now() >= at {
                    self.pending_git_at = None;
                    let folder_changed = std::mem::take(&mut self.pending_git_folder_change);
                    self.refresh_context(folder_changed);
                }
            }
            if let Some(at) = index_poll_at {
                if Instant::now() >= at {
                    self.next_index_poll = Some(Instant::now() + INDEX_POLL_INTERVAL);
                    self.poll_index();
                }
            }
        }
        self.watcher = None;
    }

    fn enabled(&self) -> bool {
        self.index.is_some() && (self.session_card || self.session_indicator)
    }

    fn ensure_index(&mut self) {
        if self.index.is_some() || self.index_failed {
            return;
        }
        match SessionIndex::open_default() {
            Ok(index) => self.index = Some(index),
            Err(e) => {
                log::error!("Session suggestions are disabled: {}", e);
                self.index_failed = true;
            },
        }
    }

    fn handle(&mut self, job: SessionSuggestionsJob) {
        match job {
            SessionSuggestionsJob::Configure {
                config,
                session_card,
                session_indicator,
            } => {
                let changed = self.config != config;
                self.config = config;
                self.session_card = session_card;
                self.session_indicator = session_indicator;
                if changed {
                    self.live_fact_cache.clear();
                }
                if !self.session_indicator {
                    self.watcher = None;
                } else if !self.subscribers.is_empty() {
                    self.ensure_watcher();
                    self.report_counts_to_subscribers();
                }
            },
            SessionSuggestionsJob::SessionStarted {
                name,
                cwd,
                card_for_client,
            } => self.session_started(name, cwd, card_for_client),
            SessionSuggestionsJob::SessionRenamed { old_name, new_name } => {
                self.ensure_index();
                if let Some(index) = self.index.as_ref() {
                    let _ = index.rename(&old_name, &new_name);
                }
                if self.session_name.as_deref() == Some(old_name.as_str()) {
                    self.session_name = Some(new_name);
                }
                self.report_counts_to_subscribers();
            },
            SessionSuggestionsJob::SessionDeleted(name) => {
                self.ensure_index();
                if let Some(index) = self.index.as_ref() {
                    let _ = index.delete(&name);
                }
                self.report_counts_to_subscribers();
            },
            SessionSuggestionsJob::FocusedPaneContext { folder, trigger } => {
                self.focused_pane_context(folder, trigger)
            },
            SessionSuggestionsJob::ClientCountChanged(count) => {
                let previous = self.attached_clients;
                self.attached_clients = count;
                if previous != count {
                    self.touch_last_used();
                }
            },
            SessionSuggestionsJob::Activity => self.touch_last_used(),
            SessionSuggestionsJob::CommandSubmitted => self.command_submitted = true,
            SessionSuggestionsJob::Subscribe {
                plugin_id,
                client_id,
            } => {
                if !self.session_indicator {
                    return;
                }
                self.ensure_index();
                self.subscribers.insert((plugin_id, client_id));
                self.ensure_watcher();
                let (counts, folder_sessions) = self.compute_counts();
                self.send_counts_to(plugin_id, client_id, counts, folder_sessions.clone());
                self.last_counts = Some(counts);
                self.last_folder_sessions = Some(folder_sessions);
            },
            SessionSuggestionsJob::Unsubscribe { plugin_id } => {
                self.subscribers.retain(|(id, _)| *id != plugin_id);
                if self.subscribers.is_empty() {
                    self.watcher = None;
                    self.last_counts = None;
                    self.last_folder_sessions = None;
                }
            },
            SessionSuggestionsJob::GetSuggestions {
                plugin_id,
                client_id,
                limit,
            } => self.send_suggestions(plugin_id, client_id, limit),
            SessionSuggestionsJob::SocketFolderChanged | SessionSuggestionsJob::IndexChanged => {
                self.report_counts_to_subscribers()
            },
            SessionSuggestionsJob::Exit => {},
        }
    }

    fn poll_index(&mut self) {
        let version = self.index.as_ref().and_then(|index| index.data_version());
        let previous = self.last_data_version;
        self.last_data_version = version;
        if previous.is_some() && version != previous {
            self.report_counts(true);
        }
    }

    fn ensure_watcher(&mut self) {
        if self.watcher.is_some() || !self.session_indicator || self.subscribers.is_empty() {
            return;
        }
        let sender = self.own_sender.clone();
        match crate::socket_folder_watcher::SocketFolderWatcher::start(
            zellij_utils::consts::ZELLIJ_SOCK_DIR.clone(),
            move || {
                let _ = sender.send(SessionSuggestionsJob::SocketFolderChanged);
            },
        ) {
            Ok(watcher) => self.watcher = Some(watcher),
            Err(e) => log::error!("Failed to watch the session socket folder: {}", e),
        }
    }

    fn session_started(
        &mut self,
        name: String,
        cwd: Option<PathBuf>,
        card_for_client: Option<ClientId>,
    ) {
        self.session_name = Some(name.clone());
        if self.session_card || self.session_indicator {
            self.ensure_index();
            if self.enabled() {
                self.record_started_session(&name, cwd);
            }
        }
        if let (Some(client_id), true) = (card_for_client, self.session_card) {
            if other_sessions_are_running(&zellij_utils::consts::ZELLIJ_SOCK_DIR, &name) {
                let _ = self
                    .senders
                    .send_to_screen(ScreenInstruction::OpenSessionCard(client_id));
            }
        }
    }

    fn record_started_session(&mut self, name: &str, cwd: Option<PathBuf>) {
        let now = now_secs();
        if let Some(index) = self.index.as_ref() {
            let _ = index.ensure_row(name, now);
            let _ = index.touch(name, now);
        }
        self.last_used_written = Some(Instant::now());
        self.context.folder = cwd;
        let start_folder = self.context.folder.clone();
        self.start_facts = Some(self.run_custom_facts(start_folder.as_deref(), FactTiming::Start));
        self.attached_clients = self.attached_clients.max(1);
        self.refresh_context(true);
    }

    fn focused_pane_context(&mut self, folder: Option<PathBuf>, trigger: ContextTrigger) {
        if !self.enabled() || self.attached_clients == 0 || self.session_name.is_none() {
            return;
        }
        let folder_changed = folder.is_some() && folder != self.context.folder;
        if folder_changed {
            self.context.folder = folder;
        } else if trigger == ContextTrigger::FolderChanged {
            return;
        }
        let now = Instant::now();
        match self.last_git_run {
            Some(last) if now.duration_since(last) < GIT_MIN_INTERVAL => {
                self.pending_git_at = Some(last + GIT_MIN_INTERVAL);
                self.pending_git_folder_change |= folder_changed;
                if folder_changed {
                    self.report_counts_to_subscribers();
                }
            },
            _ => self.refresh_context(folder_changed),
        }
    }

    fn refresh_context(&mut self, folder_changed: bool) {
        if !self.enabled() || self.attached_clients == 0 {
            return;
        }
        let Some(name) = self.session_name.clone() else {
            return;
        };
        self.last_git_run = Some(Instant::now());
        let folder = self.context.folder.clone();
        let git = folder.as_deref().and_then(run_git_rev_parse);
        self.context.repo = git.as_ref().map(|g| g.common_dir.clone());
        self.context.worktree = git.as_ref().map(|g| g.toplevel.clone());
        self.context.branch = git.as_ref().and_then(|g| g.branch.clone());
        let mut facts = BTreeMap::new();
        if let Some(hostname) = hostname() {
            facts.insert(FACT_HOSTNAME.to_owned(), hostname);
        }
        if let Some(start_facts) = &self.start_facts {
            facts.extend(start_facts.clone());
        }
        if let Some(folder) = folder.as_ref() {
            if folder_changed || !self.live_fact_cache.contains_key(folder) {
                let mut live = self.run_custom_facts(Some(folder), FactTiming::Live);
                live.extend(self.run_record_script(folder));
                self.live_fact_cache.insert(folder.clone(), live);
            }
            if let Some(live) = self.live_fact_cache.get(folder) {
                facts.extend(live.clone());
            }
        }
        self.context.facts = facts;
        let changed = self
            .index
            .as_ref()
            .and_then(|index| index.write_context(&name, &self.context, now_secs()).ok())
            .unwrap_or(false);
        if changed || folder_changed {
            self.report_counts(true);
        }
    }

    fn run_custom_facts(&self, folder: Option<&Path>, timing: FactTiming) -> BTreeMap<String, String> {
        let mut facts = BTreeMap::new();
        for fact in self.config.facts.iter().filter(|f| f.at == timing) {
            let output = run_background_command_blocking(BackgroundCommand {
                program: "sh".to_owned(),
                args: vec!["-c".to_owned(), fact.command.clone()],
                cwd: folder.map(|f| f.to_path_buf()),
                timeout: Some(FACT_TIMEOUT),
                ..Default::default()
            });
            if let Ok(output) = output {
                if output.exit_code == Some(0) {
                    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                    if !value.is_empty() {
                        facts.insert(fact.name.clone(), value);
                    }
                }
            }
        }
        facts
    }

    fn run_record_script(&self, folder: &Path) -> BTreeMap<String, String> {
        let Some(record) = self.config.record.as_ref() else {
            return BTreeMap::new();
        };
        let input = serde_json::json!({
            "session": self.session_name,
            "folder": folder.display().to_string(),
        })
        .to_string();
        match run_background_command_blocking(BackgroundCommand {
            program: expand_home(record).display().to_string(),
            cwd: Some(folder.to_path_buf()),
            input: Some(format!("{}\n", input).into_bytes()),
            timeout: Some(FACT_TIMEOUT),
            ..Default::default()
        }) {
            Ok(output) if output.exit_code == Some(0) => {
                parse_fact_lines(&String::from_utf8_lossy(&output.stdout))
            },
            _ => BTreeMap::new(),
        }
    }

    fn touch_last_used(&mut self) {
        if !self.enabled() {
            return;
        }
        let now = Instant::now();
        let due = self
            .last_used_written
            .map(|last| now.duration_since(last) >= LAST_USED_MIN_INTERVAL)
            .unwrap_or(true);
        if !due {
            return;
        }
        if let (Some(index), Some(name)) = (self.index.as_ref(), self.session_name.as_ref()) {
            let _ = index.touch(name, now_secs());
            self.last_used_written = Some(now);
        }
    }

    fn own_name(&self) -> String {
        self.session_name.clone().unwrap_or_default()
    }

    fn compute_counts(&mut self) -> (SessionCounts, Option<FolderSessions>) {
        let own_name = self.own_name();
        let rules = self.config.effective_rules();
        let context = self.context.clone();
        let Some(index) = self.index.as_mut() else {
            return (SessionCounts::default(), None);
        };
        let now = now_secs();
        let running = index
            .running_session_names()
            .into_iter()
            .filter(|n| n != &own_name)
            .count() as u32;
        let resumable_matching = index
            .matching_candidates(&context, &rules, &own_name, now)
            .map(|candidates| {
                candidates
                    .iter()
                    .filter(|c| c.state == SessionState::Resumable)
                    .count() as u32
            })
            .unwrap_or(0);
        let folder_sessions = context.folder.as_ref().and_then(|folder| {
            let rows = index.rows_exactly_in_folder(folder).ok()?;
            let candidates = index.resolve_candidates(rows, &own_name, now);
            let mut running = vec![];
            let mut resumable = vec![];
            let mut full_match_running = vec![];
            let mut sorted = candidates;
            sorted.sort_by(|a, b| b.row.last_used_at.cmp(&a.row.last_used_at));
            for candidate in sorted {
                match candidate.state {
                    SessionState::Running => {
                        if context.branch.is_some() && candidate.row.context.branch == context.branch
                        {
                            full_match_running.push(candidate.row.name.clone());
                        }
                        running.push(candidate.row.name)
                    },
                    SessionState::Resumable => resumable.push(candidate.row.name),
                }
            }
            if running.is_empty() && resumable.is_empty() {
                None
            } else {
                Some(FolderSessions {
                    folder: folder_key(folder),
                    running,
                    resumable,
                    full_match_running,
                })
            }
        });
        (
            SessionCounts {
                running,
                resumable_matching,
            },
            folder_sessions,
        )
    }

    fn send_counts_to(
        &self,
        plugin_id: u32,
        client_id: ClientId,
        counts: SessionCounts,
        folder_sessions: Option<FolderSessions>,
    ) {
        let _ = self.senders.send_to_plugin(PluginInstruction::Update(vec![
            (
                Some(plugin_id),
                Some(client_id),
                Event::SessionCountsUpdate(counts),
            ),
            (
                Some(plugin_id),
                Some(client_id),
                Event::FolderSessionsUpdate(folder_sessions.unwrap_or_default()),
            ),
        ]));
    }

    fn report_counts_to_subscribers(&mut self) {
        self.report_counts(false);
    }

    fn report_counts(&mut self, force: bool) {
        if self.subscribers.is_empty() || !self.session_indicator || self.index.is_none() {
            return;
        }
        let (counts, folder_sessions) = self.compute_counts();
        let counts_changed = force || self.last_counts != Some(counts);
        let folder_changed = force || self.last_folder_sessions.as_ref() != Some(&folder_sessions);
        if !counts_changed && !folder_changed {
            return;
        }
        let mut updates = vec![];
        for (plugin_id, client_id) in &self.subscribers {
            if counts_changed {
                updates.push((
                    Some(*plugin_id),
                    Some(*client_id),
                    Event::SessionCountsUpdate(counts),
                ));
            }
            if folder_changed {
                updates.push((
                    Some(*plugin_id),
                    Some(*client_id),
                    Event::FolderSessionsUpdate(folder_sessions.clone().unwrap_or_default()),
                ));
            }
        }
        let _ = self.senders.send_to_plugin(PluginInstruction::Update(updates));
        self.last_counts = Some(counts);
        self.last_folder_sessions = Some(folder_sessions);
    }

    fn send_suggestions(&mut self, plugin_id: u32, client_id: ClientId, limit: usize) {
        self.ensure_index();
        let own_name = self.own_name();
        let rules = self.config.effective_rules();
        let then = self.config.effective_then();
        let context = self.context.clone();
        let columns = self.config.columns();
        let column_facts: Vec<String> = columns.iter().map(|(fact, _)| fact.clone()).collect();
        let limit = limit.max(1);
        let candidate_limit = limit.max(SCRIPT_CANDIDATE_LIMIT);
        let candidates = match self.index.as_mut() {
            Some(index) => index
                .suggestion_candidates(&context, &rules, then, &own_name, candidate_limit, now_secs())
                .unwrap_or_default(),
            None => vec![],
        };
        let ranked = rank_candidates(&context, &rules, then, candidates, &column_facts);
        let base = SessionSuggestions {
            folder: context.folder.as_ref().map(|f| folder_key(f)),
            branch: context.branch.clone(),
            columns: columns
                .iter()
                .map(|(fact, title)| SessionSuggestionColumn {
                    fact: fact.clone(),
                    title: title.clone(),
                })
                .collect(),
            suggestions: vec![],
            command_submitted: self.command_submitted,
            from_script: false,
        };
        let senders = self.senders.clone();
        let send = move |senders: &ThreadSenders, suggestions: SessionSuggestions| {
            let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(
                Some(plugin_id),
                Some(client_id),
                Event::SessionSuggestions(suggestions),
            )]));
        };
        let Some(script) = self.config.script.clone() else {
            let mut result = base;
            result.suggestions = to_suggestions(&context, ranked, limit);
            send(&senders, result);
            return;
        };
        let script_candidates = script_candidates(&ranked);
        let input = script_input(&context, &script_candidates);
        let (result_sender, result_receiver) = channel();
        let script_path = expand_home(&script.path);
        let folder = context.folder.clone();
        thread::spawn(move || {
            let output = run_background_command_blocking(BackgroundCommand {
                program: script_path.display().to_string(),
                cwd: folder,
                input: Some(input.into_bytes()),
                timeout: Some(SCRIPT_HARD_TIMEOUT),
                ..Default::default()
            });
            let _ = result_sender.send(output);
        });
        let script_ordered = |output: &str| -> Option<Vec<RankedCandidate>> {
            let parsed = parse_ranking_script_output(output);
            if parsed.is_empty() {
                None
            } else {
                Some(apply_script_order(ranked.clone(), &parsed, script.hide_unlisted))
            }
        };
        match result_receiver.recv_timeout(Duration::from_millis(script.timeout_ms)) {
            Ok(Ok(output)) if output.exit_code == Some(0) => {
                let mut result = base;
                match script_ordered(&String::from_utf8_lossy(&output.stdout)) {
                    Some(ordered) => {
                        result.suggestions = to_suggestions(&context, ordered, limit);
                        result.from_script = true;
                    },
                    None => result.suggestions = to_suggestions(&context, ranked, limit),
                }
                send(&senders, result);
            },
            Ok(_) => {
                let mut result = base;
                result.suggestions = to_suggestions(&context, ranked, limit);
                send(&senders, result);
            },
            Err(_) => {
                let mut builtin = base.clone();
                builtin.suggestions = to_suggestions(&context, ranked.clone(), limit);
                send(&senders, builtin);
                let hide_unlisted = script.hide_unlisted;
                thread::spawn(move || {
                    if let Ok(Ok(output)) = result_receiver.recv() {
                        if output.exit_code != Some(0) {
                            return;
                        }
                        let parsed =
                            parse_ranking_script_output(&String::from_utf8_lossy(&output.stdout));
                        if parsed.is_empty() {
                            return;
                        }
                        let ordered = apply_script_order(ranked, &parsed, hide_unlisted);
                        let mut late = base;
                        late.suggestions = to_suggestions(&context, ordered, limit);
                        late.from_script = true;
                        send(&senders, late);
                    }
                });
            },
        }
    }
}

fn other_sessions_are_running(sock_dir: &Path, own_name: &str) -> bool {
    zellij_utils::session_index::running_session_names_in(sock_dir)
        .iter()
        .any(|name| name != own_name)
}

fn script_candidates(ranked: &[RankedCandidate]) -> Vec<RankedCandidate> {
    let mut running: Vec<RankedCandidate> = ranked
        .iter()
        .filter(|r| r.candidate.state == SessionState::Running)
        .cloned()
        .collect();
    let matching_resumable = ranked
        .iter()
        .filter(|r| r.candidate.state == SessionState::Resumable && r.tier.is_some())
        .cloned();
    let recent_resumable = ranked
        .iter()
        .filter(|r| r.candidate.state == SessionState::Resumable && r.tier.is_none())
        .cloned();
    running.extend(matching_resumable);
    running.extend(recent_resumable);
    running.truncate(SCRIPT_CANDIDATE_LIMIT);
    running
}

fn script_input(target: &SessionContext, candidates: &[RankedCandidate]) -> String {
    let mut input = String::new();
    let now = now_secs();
    for candidate in candidates {
        let row = &candidate.candidate.row;
        let line = serde_json::json!({
            "name": row.name,
            "running": candidate.candidate.state == SessionState::Running,
            "folder": row.context.folder.as_ref().map(|f| f.display().to_string()),
            "repo": row.context.repo,
            "worktree": row.context.worktree,
            "branch": row.context.branch,
            "facts": row.context.facts,
            "last_used_secs_ago": now.saturating_sub(row.last_used_at),
            "tier": candidate.tier,
            "target_folder": target.folder.as_ref().map(|f| f.display().to_string()),
            "target_repo": target.repo,
            "target_branch": target.branch,
        });
        input.push_str(&line.to_string());
        input.push('\n');
    }
    input
}

fn folder_relation(target: &SessionContext, candidate: &Candidate) -> FolderRelation {
    match (&target.folder, &candidate.row.context.folder) {
        (Some(target), Some(folder)) => {
            let target = folder_key(target);
            let folder = folder_key(folder);
            if folder == target {
                FolderRelation::Here
            } else if folder.starts_with(&target) {
                FolderRelation::Subfolder
            } else {
                FolderRelation::Other
            }
        },
        _ => FolderRelation::Other,
    }
}

fn to_suggestions(
    target: &SessionContext,
    ranked: Vec<RankedCandidate>,
    limit: usize,
) -> Vec<SessionSuggestion> {
    let now = now_secs();
    ranked
        .into_iter()
        .take(limit)
        .map(|r| {
            let relation = folder_relation(target, &r.candidate);
            let row = r.candidate.row;
            let is_running = r.candidate.state == SessionState::Running;
            let (tabs, connected_clients) = if is_running {
                live_session_layout(&row.name).unwrap_or_default()
            } else {
                (vec![], 0)
            };
            SessionSuggestion {
                name: row.name,
                is_running,
                last_used_secs_ago: if row.last_used_at == 0 {
                    0
                } else {
                    now.saturating_sub(row.last_used_at)
                },
                created_secs_ago: now.saturating_sub(row.created_at),
                folder: row.context.folder.map(|f| f.display().to_string()),
                folder_relation: relation,
                repo: row.context.repo,
                branch: row.context.branch,
                facts: row.context.facts,
                tier: r.tier.map(|t| t as u32),
                matching_columns: r.matching_facts,
                script_label: r.script_label,
                tabs,
                connected_clients,
            }
        })
        .collect()
}

fn live_session_layout(name: &str) -> Option<(Vec<SessionPreviewTab>, usize)> {
    let raw = std::fs::read_to_string(zellij_utils::consts::session_info_cache_file_name(name)).ok()?;
    let info = zellij_utils::data::SessionInfo::from_string(&raw, "").ok()?;
    Some(session_layout_from_info(&info))
}

fn session_layout_from_info(
    info: &zellij_utils::data::SessionInfo,
) -> (Vec<SessionPreviewTab>, usize) {
    let mut tab_infos: Vec<&zellij_utils::data::TabInfo> = info.tabs.iter().collect();
    tab_infos.sort_by_key(|tab| tab.position);
    let tabs = tab_infos
        .into_iter()
        .map(|tab| {
            let mut panes: Vec<&zellij_utils::data::PaneInfo> = info
                .panes
                .panes
                .get(&tab.position)
                .map(|panes| {
                    panes
                        .iter()
                        .filter(|p| p.is_selectable && !p.is_suppressed)
                        .collect()
                })
                .unwrap_or_default();
            panes.sort_by_key(|p| (p.is_floating, p.pane_y, p.pane_x));
            SessionPreviewTab {
                name: tab.name.clone(),
                active: tab.active,
                panes: panes
                    .into_iter()
                    .map(|pane| SessionPreviewPane {
                        id: pane.id,
                        is_plugin: pane.is_plugin,
                        title: pane.title.clone(),
                        focused: pane.is_focused,
                        contents: None,
                    })
                    .collect(),
            }
        })
        .collect();
    (tabs, info.connected_clients)
}

fn run_git_rev_parse(folder: &Path) -> Option<zellij_utils::session_index::GitInfo> {
    let output = run_background_command_blocking(BackgroundCommand {
        program: "git".to_owned(),
        args: vec![
            "-C".to_owned(),
            folder.display().to_string(),
            "rev-parse".to_owned(),
            "--show-toplevel".to_owned(),
            "--git-common-dir".to_owned(),
            "--abbrev-ref".to_owned(),
            "HEAD".to_owned(),
        ],
        cwd: Some(folder.to_path_buf()),
        timeout: Some(GIT_TIMEOUT),
        ..Default::default()
    })
    .ok()?;
    if output.exit_code != Some(0) {
        return None;
    }
    parse_git_rev_parse(folder, &String::from_utf8_lossy(&output.stdout))
}

pub fn expand_home(path: &Path) -> PathBuf {
    expand_home_with(path, std::env::var_os("HOME").map(PathBuf::from))
}

fn expand_home_with(path: &Path, home: Option<PathBuf>) -> PathBuf {
    let display = path.display().to_string();
    match (display.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_utils::session_index::SessionRow;

    fn ranked(name: &str, state: SessionState, tier: Option<usize>) -> RankedCandidate {
        RankedCandidate {
            candidate: Candidate {
                row: SessionRow {
                    name: name.to_owned(),
                    context: SessionContext::default(),
                    created_at: 0,
                    last_used_at: 0,
                },
                state,
            },
            tier,
            matching_facts: vec![],
            script_label: None,
        }
    }

    #[test]
    fn script_candidates_put_running_then_matching_then_recent() {
        let ranked = vec![
            ranked("recent", SessionState::Resumable, None),
            ranked("matching", SessionState::Resumable, Some(0)),
            ranked("running", SessionState::Running, None),
        ];
        let names: Vec<String> = script_candidates(&ranked)
            .into_iter()
            .map(|r| r.candidate.row.name)
            .collect();
        assert_eq!(names, vec!["running", "matching", "recent"]);
    }

    #[test]
    fn script_candidates_are_capped() {
        let ranked: Vec<RankedCandidate> = (0..150)
            .map(|i| ranked(&format!("s{}", i), SessionState::Resumable, None))
            .collect();
        assert_eq!(script_candidates(&ranked).len(), SCRIPT_CANDIDATE_LIMIT);
    }

    #[test]
    fn script_input_is_one_json_object_per_line() {
        let ranked = vec![ranked("a", SessionState::Running, Some(1))];
        let input = script_input(&SessionContext::default(), &ranked);
        let lines: Vec<&str> = input.lines().collect();
        assert_eq!(lines.len(), 1);
        let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed["name"], "a");
        assert_eq!(parsed["running"], true);
        assert_eq!(parsed["tier"], 1);
    }

    #[test]
    fn a_failing_script_command_reports_an_error() {
        let result = run_background_command_blocking(BackgroundCommand {
            program: "/definitely/not/a/real/script".to_owned(),
            timeout: Some(Duration::from_millis(500)),
            ..Default::default()
        });
        assert!(result.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_slow_command_is_killed_at_the_timeout() {
        let started = Instant::now();
        let result = run_background_command_blocking(BackgroundCommand {
            program: "sh".to_owned(),
            args: vec!["-c".to_owned(), "sleep 5".to_owned()],
            timeout: Some(Duration::from_millis(100)),
            ..Default::default()
        });
        assert!(matches!(
            result,
            Err(crate::background_jobs::BackgroundCommandError::TimedOut)
        ));
        assert!(started.elapsed() < Duration::from_secs(4));
    }

    #[cfg(unix)]
    #[test]
    fn commands_receive_their_input() {
        let result = run_background_command_blocking(BackgroundCommand {
            program: "cat".to_owned(),
            input: Some(b"hello".to_vec()),
            timeout: Some(Duration::from_secs(5)),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(result.stdout, b"hello".to_vec());
    }

    #[test]
    fn a_session_layout_lists_selectable_panes_in_screen_order() {
        use zellij_utils::data::{PaneInfo, PaneManifest, SessionInfo, TabInfo};
        let pane = |id: u32, title: &str, x: usize, y: usize| PaneInfo {
            id,
            title: title.to_owned(),
            pane_x: x,
            pane_y: y,
            is_selectable: true,
            ..Default::default()
        };
        let mut panes = std::collections::HashMap::new();
        panes.insert(
            0,
            vec![
                pane(2, "right", 50, 0),
                pane(1, "left", 0, 0),
                PaneInfo {
                    is_floating: true,
                    is_focused: true,
                    ..pane(3, "floating", 0, 0)
                },
                PaneInfo {
                    is_selectable: false,
                    ..pane(4, "tab-bar", 0, 0)
                },
                PaneInfo {
                    is_suppressed: true,
                    ..pane(5, "hidden", 0, 0)
                },
            ],
        );
        let info = SessionInfo {
            name: "s".to_owned(),
            tabs: vec![
                TabInfo {
                    position: 1,
                    name: "second".to_owned(),
                    ..Default::default()
                },
                TabInfo {
                    position: 0,
                    name: "first".to_owned(),
                    active: true,
                    ..Default::default()
                },
            ],
            panes: PaneManifest { panes },
            connected_clients: 2,
            ..Default::default()
        };
        let (tabs, clients) = session_layout_from_info(&info);
        assert_eq!(clients, 2);
        let names: Vec<&str> = tabs.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["first", "second"]);
        assert!(tabs[0].active);
        let titles: Vec<&str> = tabs[0].panes.iter().map(|p| p.title.as_str()).collect();
        assert_eq!(titles, vec!["left", "right", "floating"]);
        assert!(tabs[0].panes[2].focused);
        assert!(tabs[0].panes.iter().all(|p| p.contents.is_none()));
        assert!(tabs[1].panes.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn only_sessions_other_than_this_one_count_as_running() {
        let sock_dir = std::env::temp_dir().join(format!(
            "zellij-other-sessions-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&sock_dir);
        std::fs::create_dir_all(&sock_dir).unwrap();
        assert!(!other_sessions_are_running(&sock_dir, "me"));
        std::fs::write(sock_dir.join("me"), b"").unwrap();
        std::fs::write(sock_dir.join("web_server_bus"), b"").unwrap();
        std::fs::write(sock_dir.join("other-reply"), b"").unwrap();
        assert!(!other_sessions_are_running(&sock_dir, "me"));
        std::fs::write(sock_dir.join("other"), b"").unwrap();
        assert!(other_sessions_are_running(&sock_dir, "me"));
        let _ = std::fs::remove_dir_all(&sock_dir);
    }

    #[test]
    fn home_is_expanded() {
        let home = Some(PathBuf::from("/home/someone"));
        assert_eq!(
            expand_home_with(Path::new("~/x.sh"), home.clone()),
            PathBuf::from("/home/someone/x.sh")
        );
        assert_eq!(
            expand_home_with(Path::new("/abs.sh"), home),
            PathBuf::from("/abs.sh")
        );
        assert_eq!(
            expand_home_with(Path::new("~/x.sh"), None),
            PathBuf::from("~/x.sh")
        );
    }
}

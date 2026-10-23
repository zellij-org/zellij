use std::collections::{BTreeSet, HashMap, HashSet};

use zellij_utils::attention::{
    format_duration_ms, sanitize_display_text, AttentionCounts, AttentionDetail, AttentionItem,
    AttentionSource, AttentionStatus, CommandOutcome, CommandPhase, CommandResult, CommandState,
    CommandText, CommandTextSource, MuteScope, PaneAttention, ProgramState, ProgramStatusRecord,
    RunningCommand, SessionAttention, TabAttention,
};
use zellij_utils::data::PaneId;
use zellij_utils::input::attention::{AttentionConfig, IdleRuleScope};

use super::program_status::{ProgramStatusMessage, ProgramStatusRecords};
use super::signals::PaneSignal;

pub const STARTUP_OUTPUT_GRACE_MS: u64 = 3000;
pub const RESIZE_OUTPUT_GRACE_MS: u64 = 1500;
pub const IDLE_CHECK_THROTTLE_MS: u64 = 250;
pub const STEADY_ACTIVITY_PERCENT: u64 = 60;
const MAX_TEXT_CHARS: usize = 200;

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameMarker {
    pub status: AttentionStatus,
    pub text: String,
    pub muted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttentionEvent {
    StatusChanged(PaneId, PaneAttention),
    PromptShown(PaneId),
    CommandStarted(PaneId, Option<CommandText>),
    CommandFinished(PaneId, CommandOutcome),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Notification {
    at_ms: u64,
    title: String,
    body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Finished {
    source: AttentionSource,
    outcome: CommandOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WatchResult {
    at_ms: u64,
    text: String,
    detail: AttentionDetail,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Burst {
    start_ms: u64,
    last_ms: u64,
    active_seconds: u64,
    last_second: u64,
}

impl Burst {
    fn record(&mut self, now: u64, silence_ms: u64) {
        let second = now / 1000;
        if self.last_ms == 0 || now.saturating_sub(self.last_ms) >= silence_ms {
            *self = Burst {
                start_ms: now,
                last_ms: now,
                active_seconds: 1,
                last_second: second,
            };
        } else {
            if second != self.last_second {
                self.active_seconds += 1;
                self.last_second = second;
            }
            self.last_ms = now;
        }
    }
    fn duration_ms(&self) -> u64 {
        self.last_ms.saturating_sub(self.start_ms)
    }
    fn is_steady(&self, activity_ms: u64) -> bool {
        let duration = self.duration_ms();
        if self.last_ms == 0 || duration < activity_ms {
            return false;
        }
        let spanned_seconds = (duration / 1000).max(1);
        self.active_seconds * 100 >= spanned_seconds * STEADY_ACTIVITY_PERCENT
    }
}

#[derive(Debug, Clone, Default)]
struct PaneTrack {
    created_ms: u64,
    records: ProgramStatusRecords,
    notification: Option<Notification>,
    bell_ms: Option<u64>,
    finished: Option<Finished>,
    quiet_ms: Option<u64>,
    watch_result: Option<WatchResult>,
    command: CommandState,
    foreground: Option<RunningCommand>,
    command_pane_started_ms: Option<u64>,
    command_pane_running: Option<RunningCommand>,
    last_output_ms: Option<u64>,
    first_unseen_output_ms: Option<u64>,
    last_visible_ms: Option<u64>,
    last_focused_ms: Option<u64>,
    visible: bool,
    viewed_elsewhere: bool,
    focused: bool,
    burst: Burst,
    last_idle_schedule_ms: u64,
    muted: bool,
    ignore_output: bool,
    watch_armed_ms: Option<u64>,
    dismissed_ms: Option<u64>,
    resolved: PaneAttention,
    frame: Option<FrameMarker>,
}

impl PaneTrack {
    fn new(now: u64) -> Self {
        PaneTrack {
            created_ms: now,
            ..Default::default()
        }
    }
    fn seen(&self, at_ms: u64) -> bool {
        self.is_looked_at() || self.last_visible_ms.map(|v| v >= at_ms).unwrap_or(false)
    }
    fn is_looked_at(&self) -> bool {
        self.visible || self.viewed_elsewhere
    }
    fn acknowledged(&self, at_ms: u64) -> bool {
        self.focused || self.last_focused_ms.map(|f| f >= at_ms).unwrap_or(false)
    }
}

struct Candidate {
    status: AttentionStatus,
    source: AttentionSource,
    text: String,
    detail: AttentionDetail,
    since_ms: u64,
    past: bool,
    on_frame: bool,
    from_watch: bool,
}

fn shorten(text: &str) -> String {
    let clean = sanitize_display_text(text);
    if clean.chars().count() > MAX_TEXT_CHARS {
        let mut short: String = clean.chars().take(MAX_TEXT_CHARS - 1).collect();
        short.push('…');
        short
    } else {
        clean
    }
}

fn with_command(head: String, command: &Option<CommandText>) -> String {
    match command {
        Some(command) => format!("{}: {}", head, shorten(&command.text)),
        None => head,
    }
}

fn record_text(status: AttentionStatus, record: &ProgramStatusRecord) -> String {
    let msg = record.msg.as_deref().map(shorten);
    let title = record.title.as_deref().map(shorten);
    let body = match (title, msg) {
        (Some(t), Some(m)) if record.id.is_some() => Some(format!("{}: {}", t, m)),
        (_, Some(m)) => Some(m),
        (Some(t), None) => Some(t),
        (None, None) => None,
    };
    let head = match status {
        AttentionStatus::NeedsYou => record
            .kind
            .map(|k| k.label().to_owned())
            .unwrap_or_else(|| "needs you".to_owned()),
        AttentionStatus::Failed => "failed".to_owned(),
        AttentionStatus::Done => "done".to_owned(),
        AttentionStatus::Working => match record.progress {
            Some(p) => format!("working {}%", p),
            None => "working".to_owned(),
        },
        _ => "idle".to_owned(),
    };
    match body {
        Some(body) if status == AttentionStatus::Working && record.progress.is_none() => body,
        Some(body) => format!("{}: {}", head, body),
        None => head,
    }
}

fn outcome_head(outcome: &CommandOutcome) -> String {
    let base = match (outcome.result, outcome.exit_code) {
        (CommandResult::Failed, Some(code)) => format!("exited {}", code),
        (CommandResult::Failed, None) => "failed".to_owned(),
        (CommandResult::Succeeded, _) => "done".to_owned(),
        (CommandResult::Unknown, _) => "finished".to_owned(),
    };
    match outcome.duration_ms {
        Some(d) if d >= 1000 => format!("{} · {}", base, format_duration_ms(d)),
        _ => base,
    }
}

fn outcome_detail(outcome: &CommandOutcome) -> AttentionDetail {
    AttentionDetail {
        exit_code: outcome.exit_code,
        duration_ms: outcome.duration_ms,
        command: outcome.command.clone(),
        ..Default::default()
    }
}

#[derive(Debug, Clone)]
pub struct AttentionTracker {
    config: AttentionConfig,
    panes: HashMap<PaneId, PaneTrack>,
    pane_tabs: HashMap<PaneId, usize>,
    muted_tabs: BTreeSet<usize>,
    session_muted: bool,
    events: Vec<AttentionEvent>,
    idle_checks: Vec<(PaneId, u64)>,
    changed: BTreeSet<PaneId>,
    summary_stale: bool,
}

impl Default for AttentionTracker {
    fn default() -> Self {
        AttentionTracker::new(AttentionConfig::default())
    }
}

impl AttentionTracker {
    pub fn new(config: AttentionConfig) -> Self {
        AttentionTracker {
            config,
            panes: HashMap::new(),
            pane_tabs: HashMap::new(),
            muted_tabs: BTreeSet::new(),
            session_muted: false,
            events: vec![],
            idle_checks: vec![],
            changed: BTreeSet::new(),
            summary_stale: false,
        }
    }
    pub fn config(&self) -> &AttentionConfig {
        &self.config
    }
    pub fn set_config(&mut self, config: AttentionConfig, now: u64) {
        self.config = config;
        self.resolve_all(now);
    }
    fn done_threshold_ms(&self) -> u64 {
        self.config.done_threshold_secs() * 1000
    }
    fn activity_ms(&self) -> u64 {
        self.config.idle_activity_secs() * 1000
    }
    pub fn silence_ms(&self) -> u64 {
        (self.config.idle_silence_secs() * 1000).max(1)
    }
    fn track(&mut self, pane_id: PaneId, now: u64) -> &mut PaneTrack {
        self.panes
            .entry(pane_id)
            .or_insert_with(|| PaneTrack::new(now))
    }
    pub fn add_pane(&mut self, pane_id: PaneId, now: u64) {
        self.track(pane_id, now);
    }
    pub fn remove_pane(&mut self, pane_id: PaneId) {
        if let Some(track) = self.panes.remove(&pane_id) {
            let resolved = &track.resolved;
            if !resolved.status.is_idle() || resolved.watched || resolved.is_muted() {
                self.summary_stale = true;
            }
        }
        self.pane_tabs.remove(&pane_id);
        self.changed.remove(&pane_id);
    }
    pub fn take_summary_stale(&mut self) -> bool {
        std::mem::take(&mut self.summary_stale)
    }
    pub fn has_pane(&self, pane_id: &PaneId) -> bool {
        self.panes.contains_key(pane_id)
    }
    pub fn take_events(&mut self) -> Vec<AttentionEvent> {
        std::mem::take(&mut self.events)
    }
    pub fn take_idle_checks(&mut self) -> Vec<(PaneId, u64)> {
        std::mem::take(&mut self.idle_checks)
    }
    pub fn take_changed(&mut self) -> BTreeSet<PaneId> {
        std::mem::take(&mut self.changed)
    }
    pub fn attention(&self, pane_id: &PaneId) -> Option<&PaneAttention> {
        self.panes.get(pane_id).map(|t| &t.resolved)
    }
    pub fn frame_marker(&self, pane_id: &PaneId) -> Option<FrameMarker> {
        if !self.config.frame_markers() {
            return None;
        }
        self.panes.get(pane_id).and_then(|t| t.frame.clone())
    }
    pub fn command_state(&self, pane_id: &PaneId) -> Option<&CommandState> {
        self.panes.get(pane_id).map(|t| &t.command)
    }
    pub fn program_status_records(&self, pane_id: &PaneId) -> Vec<ProgramStatusRecord> {
        self.panes
            .get(pane_id)
            .map(|t| t.records.all())
            .unwrap_or_default()
    }
    pub fn pane_is_muted(&self, pane_id: &PaneId) -> bool {
        self.panes.get(pane_id).map(|t| t.muted).unwrap_or(false)
    }
    pub fn pane_ignores_output(&self, pane_id: &PaneId) -> bool {
        self.panes
            .get(pane_id)
            .map(|t| t.ignore_output)
            .unwrap_or(false)
    }
    pub fn pane_is_watched(&self, pane_id: &PaneId) -> bool {
        self.panes
            .get(pane_id)
            .map(|t| t.watch_armed_ms.is_some())
            .unwrap_or(false)
    }
    pub fn tab_is_muted(&self, tab_id: usize) -> bool {
        self.muted_tabs.contains(&tab_id)
    }
    pub fn session_is_muted(&self) -> bool {
        self.session_muted
    }

    pub fn unseen_output_since(&self, pane_id: &PaneId) -> Option<u64> {
        self.panes
            .get(pane_id)
            .and_then(|t| t.first_unseen_output_ms)
    }
    pub fn mark_seen(&mut self, pane_id: PaneId, now: u64) -> bool {
        let Some(track) = self.panes.get_mut(&pane_id) else {
            return false;
        };
        track.last_visible_ms = Some(now);
        track.last_focused_ms = Some(now);
        track.first_unseen_output_ms = None;
        track.dismissed_ms = Some(now);
        self.resolve(pane_id, now);
        true
    }
    pub fn mark_all_seen(&mut self, now: u64) {
        let panes: Vec<PaneId> = self.panes.keys().copied().collect();
        for pane_id in panes {
            self.mark_seen(pane_id, now);
        }
    }
    pub fn set_pane_muted(&mut self, pane_id: PaneId, muted: bool, now: u64) {
        self.track(pane_id, now).muted = muted;
        self.resolve(pane_id, now);
    }
    pub fn set_pane_ignore_output(&mut self, pane_id: PaneId, ignore: bool, now: u64) {
        self.track(pane_id, now).ignore_output = ignore;
        self.resolve(pane_id, now);
    }
    pub fn set_pane_watched(&mut self, pane_id: PaneId, watched: bool, now: u64) {
        let track = self.track(pane_id, now);
        track.watch_armed_ms = if watched { Some(now) } else { None };
        if watched {
            track.watch_result = None;
            track.burst = Burst::default();
        }
        self.resolve(pane_id, now);
    }
    pub fn set_tab_muted(&mut self, tab_id: usize, muted: bool, now: u64) {
        if muted {
            self.muted_tabs.insert(tab_id);
        } else {
            self.muted_tabs.remove(&tab_id);
        }
        let panes: Vec<PaneId> = self
            .pane_tabs
            .iter()
            .filter(|(_, t)| **t == tab_id)
            .map(|(p, _)| *p)
            .collect();
        for pane_id in panes {
            self.resolve(pane_id, now);
        }
    }
    pub fn set_session_muted(&mut self, muted: bool, now: u64) {
        self.session_muted = muted;
        self.resolve_all(now);
    }
    pub fn remove_tab(&mut self, tab_id: usize) {
        self.muted_tabs.remove(&tab_id);
    }

    pub fn sync_panes(
        &mut self,
        panes: &[(PaneId, usize)],
        visible: &HashSet<PaneId>,
        focused: &HashSet<PaneId>,
        now: u64,
    ) {
        let live: HashSet<PaneId> = panes.iter().map(|(p, _)| *p).collect();
        let gone: Vec<PaneId> = self
            .panes
            .keys()
            .filter(|p| !live.contains(p))
            .copied()
            .collect();
        for pane_id in gone {
            self.remove_pane(pane_id);
        }
        let mut to_resolve = vec![];
        for (pane_id, tab_id) in panes {
            let previous_tab = self.pane_tabs.insert(*pane_id, *tab_id);
            let track = self.track(*pane_id, now);
            let is_visible = visible.contains(pane_id);
            let is_focused = focused.contains(pane_id);
            let changed = track.visible != is_visible
                || track.focused != is_focused
                || previous_tab != Some(*tab_id);
            if track.visible && !is_visible {
                track.last_visible_ms = Some(now);
            }
            if track.focused && !is_focused {
                track.last_focused_ms = Some(now);
            }
            track.visible = is_visible;
            track.focused = is_focused;
            if is_visible {
                track.last_visible_ms = Some(now);
                track.first_unseen_output_ms = None;
            }
            if is_focused {
                track.last_focused_ms = Some(now);
            }
            if changed || is_visible {
                to_resolve.push(*pane_id);
            }
        }
        for pane_id in to_resolve {
            self.resolve(pane_id, now);
        }
    }

    pub fn set_viewed_elsewhere(&mut self, pane_id: PaneId, viewed: bool, now: u64) {
        let Some(track) = self.panes.get_mut(&pane_id) else {
            return;
        };
        if track.viewed_elsewhere == viewed {
            return;
        }
        track.viewed_elsewhere = viewed;
        track.last_visible_ms = Some(now);
        if viewed {
            track.first_unseen_output_ms = None;
        }
        self.resolve(pane_id, now);
    }

    pub fn on_output(&mut self, pane_id: PaneId, now: u64) {
        self.on_output_after_resize(pane_id, now, false);
    }

    pub fn on_output_after_resize(&mut self, pane_id: PaneId, now: u64, redraw: bool) {
        let silence_ms = self.silence_ms();
        let activity_ms = self.activity_ms();
        let idle_rule = self.config.idle_rule();
        let track = self.track(pane_id, now);
        track.last_output_ms = Some(now);
        if redraw {
            return;
        }
        if !track.is_looked_at()
            && now.saturating_sub(track.created_ms) >= STARTUP_OUTPUT_GRACE_MS
            && track.first_unseen_output_ms.is_none()
        {
            track.first_unseen_output_ms = Some(now);
        }
        track.burst.record(now, silence_ms);
        let rule_applies = Self::idle_rule_applies(track, idle_rule);
        let worth_checking = track.burst.duration_ms() * 2 >= activity_ms;
        let schedule_check = rule_applies
            && worth_checking
            && now.saturating_sub(track.last_idle_schedule_ms) >= IDLE_CHECK_THROTTLE_MS;
        if schedule_check {
            track.last_idle_schedule_ms = now;
        }
        let needs_resolve = track.first_unseen_output_ms == Some(now)
            || (track.resolved.status == AttentionStatus::Quiet);
        if schedule_check {
            self.idle_checks.push((pane_id, silence_ms));
        }
        if needs_resolve {
            self.resolve(pane_id, now);
        }
    }

    fn idle_rule_applies(track: &PaneTrack, scope: IdleRuleScope) -> bool {
        if track.watch_armed_ms.is_some() {
            return !track.command.reports_markers;
        }
        if scope == IdleRuleScope::Watched {
            return false;
        }
        track.records.is_empty() && !track.command.phase.is_at_prompt()
    }

    pub fn idle_check(&mut self, pane_id: PaneId, now: u64) {
        let silence_ms = self.silence_ms();
        let activity_ms = self.activity_ms();
        let scope = self.config.idle_rule();
        let Some(track) = self.panes.get_mut(&pane_id) else {
            return;
        };
        let Some(last_output) = track.last_output_ms else {
            return;
        };
        let quiet_for = now.saturating_sub(last_output);
        if quiet_for + 50 < silence_ms {
            if Self::idle_rule_applies(track, scope) {
                self.idle_checks
                    .push((pane_id, silence_ms.saturating_sub(quiet_for).max(1)));
            }
            return;
        }
        if !Self::idle_rule_applies(track, scope) || !track.burst.is_steady(activity_ms) {
            return;
        }
        track.burst = Burst::default();
        if track.watch_armed_ms.is_some() {
            self.fire_watch(
                pane_id,
                "went quiet".to_owned(),
                AttentionDetail::default(),
                now,
            );
        } else if !track.visible {
            track.quiet_ms = Some(now);
        }
        self.resolve(pane_id, now);
    }

    fn fire_watch(&mut self, pane_id: PaneId, text: String, detail: AttentionDetail, now: u64) {
        if let Some(track) = self.panes.get_mut(&pane_id) {
            if track.watch_armed_ms.is_some() {
                track.watch_result = Some(WatchResult {
                    at_ms: now,
                    text,
                    detail,
                });
            }
        }
    }

    pub fn on_notification(&mut self, pane_id: PaneId, title: String, body: String, now: u64) {
        let track = self.track(pane_id, now);
        track.notification = Some(Notification {
            at_ms: now,
            title,
            body,
        });
        self.resolve(pane_id, now);
    }

    pub fn on_signals(&mut self, pane_id: PaneId, signals: Vec<PaneSignal>, now: u64) {
        for signal in signals {
            self.on_signal(pane_id, signal, now);
        }
        self.resolve(pane_id, now);
    }

    fn finish_command(
        &mut self,
        pane_id: PaneId,
        source: AttentionSource,
        exit_code: Option<i32>,
        err: Option<String>,
        running: Option<RunningCommand>,
        now: u64,
    ) {
        let result = CommandResult::from_exit(exit_code, err.as_deref());
        let outcome = CommandOutcome {
            result,
            exit_code,
            err,
            duration_ms: running.as_ref().map(|r| now.saturating_sub(r.started_ms)),
            ended_ms: now,
            command: running.and_then(|r| r.command),
        };
        let track = self.track(pane_id, now);
        track.command.last = Some(outcome.clone());
        track.finished = Some(Finished {
            source,
            outcome: outcome.clone(),
        });
        self.events
            .push(AttentionEvent::CommandFinished(pane_id, outcome.clone()));
        let text = with_command(outcome_head(&outcome), &outcome.command);
        self.fire_watch(pane_id, text, outcome_detail(&outcome), now);
    }

    fn on_signal(&mut self, pane_id: PaneId, signal: PaneSignal, now: u64) {
        match signal {
            PaneSignal::ProgramStatus(message) => {
                if let ProgramStatusMessage::Query = message {
                    return;
                }
                self.track(pane_id, now).records.apply(message, now);
            },
            PaneSignal::Reset => {
                self.track(pane_id, now).records.clear_all();
            },
            PaneSignal::PromptShown { .. } => {
                let track = self.track(pane_id, now);
                track.command.reports_markers = true;
                track.records.drop_transient();
                track.foreground = None;
                let unfinished = if track.command.phase == CommandPhase::Running {
                    track.command.running.take()
                } else {
                    None
                };
                track.command.phase = CommandPhase::AtPrompt;
                track.command.running = None;
                if let Some(running) = unfinished {
                    self.finish_command(
                        pane_id,
                        AttentionSource::ShellCommand,
                        None,
                        None,
                        Some(running),
                        now,
                    );
                }
                self.events.push(AttentionEvent::PromptShown(pane_id));
            },
            PaneSignal::InputStarted => {
                let track = self.track(pane_id, now);
                track.command.reports_markers = true;
                if track.command.phase != CommandPhase::Running {
                    track.command.phase = CommandPhase::Typing;
                }
            },
            PaneSignal::CommandStarted { command, .. } => {
                let track = self.track(pane_id, now);
                track.command.reports_markers = true;
                track.command.phase = CommandPhase::Running;
                track.command.running = Some(RunningCommand {
                    command: command.clone(),
                    started_ms: now,
                });
                track.finished = None;
                track.foreground = None;
                track.burst = Burst::default();
                self.events
                    .push(AttentionEvent::CommandStarted(pane_id, command));
            },
            PaneSignal::CommandFinished { exit_code, err, .. } => {
                let track = self.track(pane_id, now);
                track.command.reports_markers = true;
                if track.command.phase != CommandPhase::Running {
                    return;
                }
                track.command.phase = CommandPhase::Finished;
                let running = track.command.running.take();
                self.finish_command(
                    pane_id,
                    AttentionSource::ShellCommand,
                    exit_code,
                    err,
                    running,
                    now,
                );
            },
            PaneSignal::Cwd(cwd) => {
                self.track(pane_id, now).command.cwd = Some(cwd);
            },
            PaneSignal::Bell => {
                self.track(pane_id, now).bell_ms = Some(now);
            },
        }
    }

    pub fn foreground_changed(&mut self, pane_id: PaneId, command: Option<String>, now: u64) {
        let track = self.track(pane_id, now);
        if track.command.reports_markers || track.command_pane_started_ms.is_some() {
            return;
        }
        match (command, track.foreground.take()) {
            (Some(text), None) => {
                let command = super::shell::clean_command_text(&text).map(|text| CommandText {
                    text,
                    source: CommandTextSource::Process,
                });
                track.foreground = Some(RunningCommand {
                    command: command.clone(),
                    started_ms: now,
                });
                track.command.phase = CommandPhase::Running;
                track.command.running = track.foreground.clone();
                track.finished = None;
                self.events
                    .push(AttentionEvent::CommandStarted(pane_id, command));
            },
            (Some(_), Some(running)) => {
                track.foreground = Some(running);
            },
            (None, Some(running)) => {
                track.command.phase = CommandPhase::Finished;
                track.command.running = None;
                self.finish_command(
                    pane_id,
                    AttentionSource::ForegroundProcess,
                    None,
                    None,
                    Some(running),
                    now,
                );
            },
            (None, None) => {},
        }
        self.resolve(pane_id, now);
    }

    pub fn command_pane_started(&mut self, pane_id: PaneId, command: Option<String>, now: u64) {
        let track = self.track(pane_id, now);
        track.command_pane_started_ms = Some(now);
        let command = command
            .and_then(|c| super::shell::clean_command_text(&c))
            .map(|text| CommandText {
                text,
                source: CommandTextSource::Process,
            });
        track.command_pane_running = Some(RunningCommand {
            command,
            started_ms: now,
        });
        track.finished = None;
        if !track.command.reports_markers {
            track.command.phase = CommandPhase::Running;
            track.command.running = track.command_pane_running.clone();
        }
        self.resolve(pane_id, now);
    }

    pub fn pane_exited(&mut self, pane_id: PaneId, exit_status: Option<i32>, now: u64) {
        let Some(track) = self.panes.get_mut(&pane_id) else {
            return;
        };
        track.records.drop_transient();
        let running = track.command_pane_running.take();
        if running.is_some() || track.command_pane_started_ms.is_some() {
            track.command.phase = CommandPhase::Finished;
            track.command.running = None;
            let running = running.or_else(|| {
                track
                    .command_pane_started_ms
                    .map(|started_ms| RunningCommand {
                        command: None,
                        started_ms,
                    })
            });
            let exit_code = exit_status;
            let err = if exit_status.is_none() {
                Some("signal".to_owned())
            } else {
                None
            };
            self.finish_command(
                pane_id,
                AttentionSource::CommandPane,
                exit_code,
                err,
                running,
                now,
            );
        } else {
            let detail = AttentionDetail {
                exit_code: exit_status,
                ..Default::default()
            };
            let text = match exit_status {
                Some(code) => format!("exited {}", code),
                None => "exited".to_owned(),
            };
            self.fire_watch(pane_id, text, detail, now);
        }
        self.resolve(pane_id, now);
    }

    fn mute_scope(&self, pane_id: &PaneId, track: &PaneTrack) -> Option<MuteScope> {
        if track.muted {
            Some(MuteScope::Pane)
        } else if self
            .pane_tabs
            .get(pane_id)
            .map(|t| self.muted_tabs.contains(t))
            .unwrap_or(false)
        {
            Some(MuteScope::Tab)
        } else if self.session_muted {
            Some(MuteScope::Session)
        } else {
            None
        }
    }

    fn candidates(&self, track: &PaneTrack) -> Vec<Candidate> {
        let mut candidates = vec![];
        let has_records = !track.records.is_empty();
        if let Some((status, record)) = track.records.deciding_record() {
            if !status.is_idle() {
                let past = matches!(record.state(), ProgramState::Done | ProgramState::Error);
                candidates.push(Candidate {
                    status,
                    source: AttentionSource::ProgramStatus,
                    text: record_text(status, &record),
                    detail: AttentionDetail {
                        kind: record.kind,
                        progress: record.progress,
                        app: record.app.clone(),
                        title: record.title.clone(),
                        body: record.msg.clone(),
                        duration_ms: past.then(|| track.records.worked_ms(&record.id)).flatten(),
                        ..Default::default()
                    },
                    since_ms: record.updated_ms,
                    past,
                    on_frame: true,
                    from_watch: false,
                });
            }
        }
        if let Some(watch) = &track.watch_result {
            candidates.push(Candidate {
                status: AttentionStatus::NeedsYou,
                source: AttentionSource::Watch,
                text: watch.text.clone(),
                detail: watch.detail.clone(),
                since_ms: watch.at_ms,
                past: true,
                on_frame: true,
                from_watch: true,
            });
        }
        if let Some(notification) = &track.notification {
            let title = shorten(&notification.title);
            let body = shorten(&notification.body);
            let text = match (title.is_empty(), body.is_empty()) {
                (false, false) => format!("{}: {}", title, body),
                (false, true) => title,
                (true, false) => body,
                (true, true) => "notification".to_owned(),
            };
            candidates.push(Candidate {
                status: AttentionStatus::NeedsYou,
                source: AttentionSource::Notification,
                text,
                detail: AttentionDetail {
                    title: Some(notification.title.clone()).filter(|t| !t.is_empty()),
                    body: Some(notification.body.clone()).filter(|b| !b.is_empty()),
                    ..Default::default()
                },
                since_ms: notification.at_ms,
                past: true,
                on_frame: true,
                from_watch: false,
            });
        }
        if let Some(bell_ms) = track.bell_ms {
            candidates.push(Candidate {
                status: AttentionStatus::NeedsYou,
                source: AttentionSource::Bell,
                text: "bell".to_owned(),
                detail: AttentionDetail::default(),
                since_ms: bell_ms,
                past: true,
                on_frame: true,
                from_watch: false,
            });
        }
        if let Some(finished) = &track.finished {
            let outcome = &finished.outcome;
            let long_enough = outcome
                .duration_ms
                .map(|d| d >= self.done_threshold_ms())
                .unwrap_or(false);
            let status = match outcome.result {
                CommandResult::Failed => Some(AttentionStatus::Failed),
                CommandResult::Succeeded | CommandResult::Unknown if long_enough => {
                    Some(AttentionStatus::Done)
                },
                _ => None,
            };
            if let Some(status) = status {
                candidates.push(Candidate {
                    status,
                    source: finished.source,
                    text: with_command(outcome_head(outcome), &outcome.command),
                    detail: outcome_detail(outcome),
                    since_ms: outcome.ended_ms,
                    past: true,
                    on_frame: finished.source != AttentionSource::CommandPane,
                    from_watch: false,
                });
            }
        }
        if let Some(quiet_ms) = track.quiet_ms {
            if !has_records {
                candidates.push(Candidate {
                    status: AttentionStatus::Quiet,
                    source: AttentionSource::IdleRule,
                    text: "went quiet".to_owned(),
                    detail: AttentionDetail::default(),
                    since_ms: quiet_ms,
                    past: true,
                    on_frame: false,
                    from_watch: false,
                });
            }
        }
        if let Some(first_unseen) = track.first_unseen_output_ms {
            let watched = track.watch_armed_ms.is_some();
            if !track.ignore_output && !track.is_looked_at() && (watched || !has_records) {
                candidates.push(Candidate {
                    status: AttentionStatus::NewOutput,
                    source: AttentionSource::UnseenOutput,
                    text: "new output".to_owned(),
                    detail: AttentionDetail::default(),
                    since_ms: first_unseen,
                    past: true,
                    on_frame: watched,
                    from_watch: false,
                });
            }
        }
        if let Some(dismissed_ms) = track.dismissed_ms {
            candidates.retain(|c| c.past || c.since_ms > dismissed_ms);
        }
        candidates
    }

    fn pick<'a>(
        candidates: &'a [Candidate],
        hidden: impl Fn(&Candidate) -> bool,
    ) -> Option<&'a Candidate> {
        let mut best: Option<&Candidate> = None;
        for candidate in candidates {
            if hidden(candidate) {
                continue;
            }
            match best {
                Some(b) if b.status >= candidate.status => {},
                _ => best = Some(candidate),
            }
        }
        best
    }

    pub fn resolve_all(&mut self, now: u64) {
        let panes: Vec<PaneId> = self.panes.keys().copied().collect();
        for pane_id in panes {
            self.resolve(pane_id, now);
        }
    }

    pub fn resolve(&mut self, pane_id: PaneId, now: u64) {
        let Some(track) = self.panes.get(&pane_id) else {
            return;
        };
        let candidates = self.candidates(track);
        let muted = self.mute_scope(&pane_id, track);
        let best = Self::pick(&candidates, |c| c.past && track.seen(c.since_ms));
        let previous = track.resolved.clone();
        let mut resolved = match best {
            Some(c) => PaneAttention {
                status: c.status,
                source: Some(c.source),
                text: c.text.clone(),
                detail: c.detail.clone(),
                since_ms: c.since_ms,
                changed_ms: previous.changed_ms,
                seen: track.seen(c.since_ms),
                muted,
                ignore_output: track.ignore_output,
                watched: track.watch_armed_ms.is_some(),
                from_watch: c.from_watch,
            },
            None => PaneAttention {
                muted,
                ignore_output: track.ignore_output,
                watched: track.watch_armed_ms.is_some(),
                changed_ms: previous.changed_ms,
                ..Default::default()
            },
        };
        if resolved != previous {
            resolved.changed_ms = now;
        }
        let frame_best = Self::pick(&candidates, |c| {
            !c.on_frame || (c.past && track.acknowledged(c.since_ms))
        });
        let frame = match (muted, frame_best) {
            (Some(MuteScope::Pane), _) | (Some(_), Some(_)) => Some(FrameMarker {
                status: frame_best
                    .map(|c| c.status)
                    .unwrap_or(AttentionStatus::NewOutput),
                text: "muted".to_owned(),
                muted: true,
            }),
            (None, Some(c)) => Some(FrameMarker {
                status: c.status,
                text: c.text.clone(),
                muted: false,
            }),
            (_, None) => None,
        };
        let status_changed = resolved != previous;
        let frame_changed = frame != track.frame;
        let track = self.panes.get_mut(&pane_id).unwrap();
        if status_changed {
            track.resolved = resolved.clone();
            let meaningful = resolved.status != previous.status
                || resolved.text != previous.text
                || resolved.muted != previous.muted
                || resolved.ignore_output != previous.ignore_output
                || resolved.watched != previous.watched
                || resolved.seen != previous.seen
                || resolved.detail != previous.detail;
            if meaningful {
                self.events
                    .push(AttentionEvent::StatusChanged(pane_id, resolved));
            }
        }
        if frame_changed {
            track.frame = frame;
        }
        if status_changed || frame_changed {
            self.changed.insert(pane_id);
        }
        self.prune_seen(pane_id);
    }

    fn prune_seen(&mut self, pane_id: PaneId) {
        let Some(track) = self.panes.get_mut(&pane_id) else {
            return;
        };
        let seen_and_acknowledged =
            |track: &PaneTrack, at: u64| track.seen(at) && track.acknowledged(at);
        if let Some(at) = track.bell_ms {
            if seen_and_acknowledged(track, at) {
                track.bell_ms = None;
            }
        }
        if let Some(at) = track.notification.as_ref().map(|n| n.at_ms) {
            if seen_and_acknowledged(track, at) {
                track.notification = None;
            }
        }
        if let Some(at) = track.watch_result.as_ref().map(|w| w.at_ms) {
            if track.seen(at) {
                track.watch_result = None;
            }
        }
        if let Some(at) = track.quiet_ms {
            if track.seen(at) {
                track.quiet_ms = None;
            }
        }
    }

    pub fn tab_attention(&self, tab_id: usize, panes: &[PaneId]) -> TabAttention {
        let mut counts = AttentionCounts::default();
        for pane_id in panes {
            if let Some(track) = self.panes.get(pane_id) {
                counts.add(&track.resolved);
            }
        }
        TabAttention {
            status: counts.highest_counted(),
            counts,
            muted: self.muted_tabs.contains(&tab_id),
        }
    }

    pub fn session_attention(
        &self,
        tabs: &[(usize, String, Vec<(PaneId, String)>)],
    ) -> SessionAttention {
        let mut counts = AttentionCounts::default();
        let mut items = vec![];
        for (tab_id, tab_name, panes) in tabs {
            for (pane_id, pane_title) in panes {
                let Some(track) = self.panes.get(pane_id) else {
                    continue;
                };
                counts.add(&track.resolved);
                if !track.resolved.status.is_idle() || track.resolved.watched {
                    items.push(AttentionItem {
                        pane_id: *pane_id,
                        tab_id: *tab_id,
                        tab_name: tab_name.clone(),
                        pane_title: pane_title.clone(),
                        attention: track.resolved.clone(),
                    });
                }
            }
        }
        items.sort_by(|a, b| {
            b.attention
                .effective_status()
                .cmp(&a.attention.effective_status())
                .then(b.attention.since_ms.cmp(&a.attention.since_ms))
        });
        let top = items
            .iter()
            .find(|i| !i.attention.is_muted() && i.attention.status.is_counted())
            .cloned();
        SessionAttention {
            status: counts.highest_counted(),
            counts,
            muted: self.session_muted,
            top,
            items,
        }
    }
}

#[cfg(test)]
#[path = "./unit/tracker_tests.rs"]
mod tracker_tests;

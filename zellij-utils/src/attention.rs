use serde::{Deserialize, Serialize};
use std::fmt;

use crate::data::PaneId;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
pub enum AttentionStatus {
    #[default]
    Idle,
    Working,
    NewOutput,
    Quiet,
    Done,
    Failed,
    NeedsYou,
}

impl AttentionStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            AttentionStatus::NeedsYou => "●",
            AttentionStatus::Failed => "✗",
            AttentionStatus::Done => "✓",
            AttentionStatus::Quiet => "◌",
            AttentionStatus::NewOutput => "•",
            AttentionStatus::Working => "◐",
            AttentionStatus::Idle => "",
        }
    }
    pub fn letter(&self) -> &'static str {
        match self {
            AttentionStatus::NeedsYou => "!",
            AttentionStatus::Failed => "x",
            AttentionStatus::Done => "v",
            AttentionStatus::Quiet => "~",
            AttentionStatus::NewOutput => "*",
            AttentionStatus::Working => ">",
            AttentionStatus::Idle => "",
        }
    }
    pub fn marker(&self, simplified_ui: bool) -> &'static str {
        if simplified_ui {
            self.letter()
        } else {
            self.symbol()
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            AttentionStatus::NeedsYou => "needs_you",
            AttentionStatus::Failed => "failed",
            AttentionStatus::Done => "done",
            AttentionStatus::Quiet => "quiet",
            AttentionStatus::NewOutput => "new_output",
            AttentionStatus::Working => "working",
            AttentionStatus::Idle => "idle",
        }
    }
    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "needs_you" => AttentionStatus::NeedsYou,
            "failed" => AttentionStatus::Failed,
            "done" => AttentionStatus::Done,
            "quiet" => AttentionStatus::Quiet,
            "new_output" => AttentionStatus::NewOutput,
            "working" => AttentionStatus::Working,
            "idle" => AttentionStatus::Idle,
            _ => return None,
        })
    }
    pub fn is_counted(&self) -> bool {
        matches!(
            self,
            AttentionStatus::NeedsYou
                | AttentionStatus::Failed
                | AttentionStatus::Done
                | AttentionStatus::Working
        )
    }
    pub fn is_idle(&self) -> bool {
        *self == AttentionStatus::Idle
    }
}

impl fmt::Display for AttentionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AttentionSource {
    ProgramStatus,
    Notification,
    Bell,
    CommandPane,
    ShellCommand,
    ForegroundProcess,
    IdleRule,
    Watch,
    UnseenOutput,
}

impl AttentionSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            AttentionSource::ProgramStatus => "program_status",
            AttentionSource::Notification => "notification",
            AttentionSource::Bell => "bell",
            AttentionSource::CommandPane => "command_pane",
            AttentionSource::ShellCommand => "shell_command",
            AttentionSource::ForegroundProcess => "foreground_process",
            AttentionSource::IdleRule => "idle_rule",
            AttentionSource::Watch => "watch",
            AttentionSource::UnseenOutput => "unseen_output",
        }
    }
    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "program_status" => AttentionSource::ProgramStatus,
            "notification" => AttentionSource::Notification,
            "bell" => AttentionSource::Bell,
            "command_pane" => AttentionSource::CommandPane,
            "shell_command" => AttentionSource::ShellCommand,
            "foreground_process" => AttentionSource::ForegroundProcess,
            "idle_rule" => AttentionSource::IdleRule,
            "watch" => AttentionSource::Watch,
            "unseen_output" => AttentionSource::UnseenOutput,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlockedKind {
    Permission,
    Question,
    Auth,
}

impl BlockedKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            BlockedKind::Permission => "permission",
            BlockedKind::Question => "question",
            BlockedKind::Auth => "auth",
        }
    }
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "permission" => Some(BlockedKind::Permission),
            "question" => Some(BlockedKind::Question),
            "auth" => Some(BlockedKind::Auth),
            _ => None,
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            BlockedKind::Permission => "needs approval",
            BlockedKind::Question => "has a question",
            BlockedKind::Auth => "needs login",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProgramState {
    Idle,
    Working,
    Done,
    Blocked,
    Error,
}

impl ProgramState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProgramState::Idle => "idle",
            ProgramState::Working => "working",
            ProgramState::Done => "done",
            ProgramState::Blocked => "blocked",
            ProgramState::Error => "error",
        }
    }
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "idle" => Some(ProgramState::Idle),
            "working" => Some(ProgramState::Working),
            "done" => Some(ProgramState::Done),
            "blocked" => Some(ProgramState::Blocked),
            "error" => Some(ProgramState::Error),
            _ => None,
        }
    }
    pub fn attention_status(&self) -> AttentionStatus {
        match self {
            ProgramState::Idle => AttentionStatus::Idle,
            ProgramState::Working => AttentionStatus::Working,
            ProgramState::Done => AttentionStatus::Done,
            ProgramState::Blocked => AttentionStatus::NeedsYou,
            ProgramState::Error => AttentionStatus::Failed,
        }
    }
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            ProgramState::Idle | ProgramState::Working | ProgramState::Blocked
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct ProgramStatusRecord {
    pub id: Option<String>,
    pub state: Option<ProgramState>,
    pub kind: Option<BlockedKind>,
    pub progress: Option<u8>,
    pub app: Option<String>,
    pub title: Option<String>,
    pub msg: Option<String>,
    pub updated_ms: u64,
}

impl ProgramStatusRecord {
    pub fn is_root(&self) -> bool {
        self.id.is_none()
    }
    pub fn state(&self) -> ProgramState {
        self.state.unwrap_or(ProgramState::Idle)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CommandTextSource {
    Shell,
    Screen,
    Process,
}

impl CommandTextSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommandTextSource::Shell => "shell",
            CommandTextSource::Screen => "screen",
            CommandTextSource::Process => "process",
        }
    }
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "shell" => Some(CommandTextSource::Shell),
            "screen" => Some(CommandTextSource::Screen),
            "process" => Some(CommandTextSource::Process),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommandText {
    pub text: String,
    pub source: CommandTextSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CommandResult {
    Succeeded,
    Failed,
    Unknown,
}

impl CommandResult {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommandResult::Succeeded => "succeeded",
            CommandResult::Failed => "failed",
            CommandResult::Unknown => "unknown",
        }
    }
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "succeeded" => Some(CommandResult::Succeeded),
            "failed" => Some(CommandResult::Failed),
            "unknown" => Some(CommandResult::Unknown),
            _ => None,
        }
    }
    pub fn from_exit(exit_code: Option<i32>, err: Option<&str>) -> Self {
        if err.map(|e| !e.is_empty()).unwrap_or(false) {
            return CommandResult::Failed;
        }
        match exit_code {
            Some(0) => CommandResult::Succeeded,
            Some(_) => CommandResult::Failed,
            None if err.is_some() => CommandResult::Succeeded,
            None => CommandResult::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommandOutcome {
    pub result: CommandResult,
    pub exit_code: Option<i32>,
    pub err: Option<String>,
    pub duration_ms: Option<u64>,
    pub ended_ms: u64,
    pub command: Option<CommandText>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunningCommand {
    pub command: Option<CommandText>,
    pub started_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum CommandPhase {
    #[default]
    Unknown,
    AtPrompt,
    Typing,
    Running,
    Finished,
}

impl CommandPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommandPhase::Unknown => "unknown",
            CommandPhase::AtPrompt => "at_prompt",
            CommandPhase::Typing => "typing",
            CommandPhase::Running => "running",
            CommandPhase::Finished => "finished",
        }
    }
    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "unknown" => CommandPhase::Unknown,
            "at_prompt" => CommandPhase::AtPrompt,
            "typing" => CommandPhase::Typing,
            "running" => CommandPhase::Running,
            "finished" => CommandPhase::Finished,
            _ => return None,
        })
    }
    pub fn is_at_prompt(&self) -> bool {
        matches!(self, CommandPhase::AtPrompt | CommandPhase::Typing)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CwdSource {
    Osc7,
    KittyShellCwd,
    ConEmu,
    ITerm2,
    Vscode,
    Poll,
}

impl CwdSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            CwdSource::Osc7 => "osc7",
            CwdSource::KittyShellCwd => "kitty_shell_cwd",
            CwdSource::ConEmu => "conemu",
            CwdSource::ITerm2 => "iterm2",
            CwdSource::Vscode => "vscode",
            CwdSource::Poll => "poll",
        }
    }
    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "osc7" => CwdSource::Osc7,
            "kitty_shell_cwd" => CwdSource::KittyShellCwd,
            "conemu" => CwdSource::ConEmu,
            "iterm2" => CwdSource::ITerm2,
            "vscode" => CwdSource::Vscode,
            "poll" => CwdSource::Poll,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReportedCwd {
    pub path: String,
    pub source: CwdSource,
    pub host: Option<String>,
    pub is_local: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct CommandState {
    pub phase: CommandPhase,
    pub running: Option<RunningCommand>,
    pub last: Option<CommandOutcome>,
    pub cwd: Option<ReportedCwd>,
    pub reports_markers: bool,
}

impl CommandState {
    pub fn without_command_text(mut self) -> Self {
        if let Some(running) = self.running.as_mut() {
            running.command = None;
        }
        if let Some(last) = self.last.as_mut() {
            last.command = None;
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct AttentionDetail {
    pub kind: Option<BlockedKind>,
    pub progress: Option<u8>,
    pub app: Option<String>,
    pub title: Option<String>,
    pub body: Option<String>,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    pub command: Option<CommandText>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MuteScope {
    Pane,
    Tab,
    Session,
}

impl MuteScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            MuteScope::Pane => "pane",
            MuteScope::Tab => "tab",
            MuteScope::Session => "session",
        }
    }
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "pane" => Some(MuteScope::Pane),
            "tab" => Some(MuteScope::Tab),
            "session" => Some(MuteScope::Session),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct PaneAttention {
    pub status: AttentionStatus,
    pub source: Option<AttentionSource>,
    pub text: String,
    pub detail: AttentionDetail,
    pub since_ms: u64,
    pub changed_ms: u64,
    pub seen: bool,
    pub muted: Option<MuteScope>,
    pub ignore_output: bool,
    pub watched: bool,
    pub from_watch: bool,
}

impl PaneAttention {
    pub fn is_muted(&self) -> bool {
        self.muted.is_some()
    }
    pub fn effective_status(&self) -> AttentionStatus {
        if self.is_muted() {
            AttentionStatus::Idle
        } else {
            self.status
        }
    }
    pub fn without_command_text(mut self) -> Self {
        self.detail.command = None;
        if matches!(
            self.source,
            Some(AttentionSource::ShellCommand)
                | Some(AttentionSource::CommandPane)
                | Some(AttentionSource::ForegroundProcess)
                | Some(AttentionSource::Watch)
        ) {
            self.text = strip_command_from_text(&self.text);
        }
        self
    }
}

pub fn strip_command_from_text(text: &str) -> String {
    match text.split_once(": ") {
        Some((head, _)) => head.to_owned(),
        None => text.to_owned(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct AttentionCounts {
    pub needs_you: usize,
    pub failed: usize,
    pub done: usize,
    pub working: usize,
    pub quiet: usize,
    pub new_output: usize,
    pub muted: usize,
}

impl AttentionCounts {
    pub fn add(&mut self, attention: &PaneAttention) {
        if attention.is_muted() {
            if !attention.status.is_idle() {
                self.muted += 1;
            }
            return;
        }
        match attention.status {
            AttentionStatus::NeedsYou => self.needs_you += 1,
            AttentionStatus::Failed => self.failed += 1,
            AttentionStatus::Done => self.done += 1,
            AttentionStatus::Working => self.working += 1,
            AttentionStatus::Quiet => self.quiet += 1,
            AttentionStatus::NewOutput => self.new_output += 1,
            AttentionStatus::Idle => {},
        }
    }
    pub fn merge(&mut self, other: &AttentionCounts) {
        self.needs_you += other.needs_you;
        self.failed += other.failed;
        self.done += other.done;
        self.working += other.working;
        self.quiet += other.quiet;
        self.new_output += other.new_output;
        self.muted += other.muted;
    }
    pub fn highest_counted(&self) -> AttentionStatus {
        if self.needs_you > 0 {
            AttentionStatus::NeedsYou
        } else if self.failed > 0 {
            AttentionStatus::Failed
        } else if self.done > 0 {
            AttentionStatus::Done
        } else if self.working > 0 {
            AttentionStatus::Working
        } else {
            AttentionStatus::Idle
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AttentionItem {
    pub pane_id: PaneId,
    pub tab_id: usize,
    pub tab_name: String,
    pub pane_title: String,
    pub attention: PaneAttention,
}

impl AttentionItem {
    pub fn label(&self) -> String {
        let pane = sanitize_display_text(&self.pane_title);
        let tab = sanitize_display_text(&self.tab_name);
        let text = sanitize_display_text(&self.attention.text);
        let place = if tab.is_empty() {
            pane
        } else if pane.is_empty() {
            tab
        } else {
            format!("{} › {}", tab, pane)
        };
        if text.is_empty() {
            place
        } else {
            format!("{}: {}", place, text)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct TabAttention {
    pub status: AttentionStatus,
    pub counts: AttentionCounts,
    pub muted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct SessionAttention {
    pub status: AttentionStatus,
    pub counts: AttentionCounts,
    pub muted: bool,
    pub top: Option<AttentionItem>,
    pub items: Vec<AttentionItem>,
}

impl SessionAttention {
    pub fn without_command_text(mut self) -> Self {
        if let Some(top) = self.top.as_mut() {
            top.attention = top.attention.clone().without_command_text();
        }
        for item in self.items.iter_mut() {
            item.attention = item.attention.clone().without_command_text();
        }
        self
    }
}

impl AttentionCounts {
    pub fn of(&self, status: AttentionStatus) -> usize {
        match status {
            AttentionStatus::NeedsYou => self.needs_you,
            AttentionStatus::Failed => self.failed,
            AttentionStatus::Done => self.done,
            AttentionStatus::Working => self.working,
            AttentionStatus::Quiet => self.quiet,
            AttentionStatus::NewOutput => self.new_output,
            AttentionStatus::Idle => 0,
        }
    }
}

pub const CURRENT_SESSION_LABEL: &str = "here";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct GlobalSessionAttention {
    pub name: String,
    pub is_current: bool,
    pub attention: SessionAttention,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GlobalAttentionItem {
    pub session: String,
    pub is_current: bool,
    pub item: AttentionItem,
}

impl GlobalAttentionItem {
    pub fn is_counted(&self) -> bool {
        !self.item.attention.is_muted() && self.item.attention.status.is_counted()
    }
    pub fn is_muted(&self) -> bool {
        self.item.attention.is_muted()
    }
    pub fn is_activity(&self) -> bool {
        !self.is_muted()
            && matches!(
                self.item.attention.status,
                AttentionStatus::Quiet | AttentionStatus::NewOutput
            )
    }
    pub fn session_label(&self) -> String {
        if self.is_current {
            CURRENT_SESSION_LABEL.to_owned()
        } else {
            sanitize_display_text(&self.session)
        }
    }
    pub fn pane_label(&self) -> String {
        let pane = sanitize_display_text(&self.item.pane_title);
        if pane.is_empty() {
            sanitize_display_text(&self.item.tab_name)
        } else {
            pane
        }
    }
    pub fn place(&self) -> String {
        format!("{} {}", self.session_label(), self.pane_label())
    }
    pub fn text(&self) -> String {
        sanitize_display_text(&self.item.attention.text)
    }
    pub fn is_same_pane(&self, session: &str, pane_id: &PaneId) -> bool {
        self.session == session && &self.item.pane_id == pane_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct GlobalAttention {
    pub current_session: String,
    pub sessions: Vec<GlobalSessionAttention>,
    pub items: Vec<GlobalAttentionItem>,
    pub counts: AttentionCounts,
    pub status: AttentionStatus,
}

impl GlobalAttention {
    pub fn build(current_session: &str, sessions: Vec<(String, SessionAttention)>) -> Self {
        let mut sessions: Vec<GlobalSessionAttention> = sessions
            .into_iter()
            .map(|(name, attention)| GlobalSessionAttention {
                is_current: name == current_session,
                name,
                attention,
            })
            .collect();
        sessions.sort_by(|a, b| b.is_current.cmp(&a.is_current).then(a.name.cmp(&b.name)));
        sessions.dedup_by(|a, b| a.name == b.name);
        let mut counts = AttentionCounts::default();
        let mut items = vec![];
        for session in &sessions {
            counts.merge(&session.attention.counts);
            for item in &session.attention.items {
                items.push(GlobalAttentionItem {
                    session: session.name.clone(),
                    is_current: session.is_current,
                    item: item.clone(),
                });
            }
        }
        items.sort_by(|a, b| {
            b.item
                .attention
                .effective_status()
                .cmp(&a.item.attention.effective_status())
                .then(b.item.attention.since_ms.cmp(&a.item.attention.since_ms))
                .then(b.is_current.cmp(&a.is_current))
                .then(a.session.cmp(&b.session))
        });
        GlobalAttention {
            current_session: current_session.to_owned(),
            sessions,
            items,
            status: counts.highest_counted(),
            counts,
        }
    }
    pub fn running(&self) -> usize {
        self.sessions.len()
    }
    pub fn has_session(&self, name: &str) -> bool {
        self.sessions.iter().any(|s| s.name == name)
    }
    pub fn session(&self, name: &str) -> Option<&GlobalSessionAttention> {
        self.sessions.iter().find(|s| s.name == name)
    }
    pub fn top(&self) -> Option<(AttentionStatus, usize)> {
        let status = self.counts.highest_counted();
        if status.is_idle() {
            None
        } else {
            Some((status, self.counts.of(status)))
        }
    }
    pub fn counted_items(&self) -> Vec<&GlobalAttentionItem> {
        self.items.iter().filter(|i| i.is_counted()).collect()
    }
    pub fn activity_items(&self) -> Vec<&GlobalAttentionItem> {
        self.items.iter().filter(|i| i.is_activity()).collect()
    }
    pub fn muted_items(&self) -> Vec<&GlobalAttentionItem> {
        self.items.iter().filter(|i| i.is_muted()).collect()
    }
    pub fn find(&self, session: &str, pane_id: &PaneId) -> Option<&GlobalAttentionItem> {
        self.items.iter().find(|i| i.is_same_pane(session, pane_id))
    }
    pub fn next_counted_after(
        &self,
        session: &str,
        pane_id: Option<&PaneId>,
    ) -> Option<&GlobalAttentionItem> {
        let counted = self.counted_items();
        if counted.is_empty() {
            return None;
        }
        let position = pane_id.and_then(|pane_id| {
            counted
                .iter()
                .position(|item| item.is_same_pane(session, pane_id))
        });
        match position {
            Some(_) if counted.len() == 1 => None,
            Some(position) => Some(counted[(position + 1) % counted.len()]),
            None => Some(counted[0]),
        }
    }
    pub fn without_command_text(mut self) -> Self {
        for session in self.sessions.iter_mut() {
            session.attention = std::mem::take(&mut session.attention).without_command_text();
        }
        for item in self.items.iter_mut() {
            item.item.attention = item.item.attention.clone().without_command_text();
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HostProgramStatus {
    pub state: String,
    pub kind: Option<String>,
    pub progress: Option<u8>,
    pub msg: String,
}

pub const HOST_STATUS_APP: &str = "zellij";
pub const HOST_STATUS_MAX_MSG_BYTES: usize = 2048;

#[cfg(not(target_family = "wasm"))]
fn truncate_utf8(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(not(target_family = "wasm"))]
impl HostProgramStatus {
    pub fn to_osc(&self) -> String {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine as _;
        let mut body = format!("state={}:app={}", self.state, HOST_STATUS_APP);
        if let Some(kind) = &self.kind {
            body.push_str(&format!(":kind={}", kind));
        }
        if let Some(progress) = self.progress {
            body.push_str(&format!(":progress={}", progress.min(100)));
        }
        let msg = sanitize_display_text(&self.msg);
        let msg = truncate_utf8(&msg, HOST_STATUS_MAX_MSG_BYTES);
        if !msg.is_empty() {
            body.push_str(&format!(":msg={}", STANDARD.encode(msg)));
        }
        format!("\u{1b}]7501;{}\u{1b}\\", body)
    }
    pub fn clear_osc() -> String {
        "\u{1b}]7501;state=clear\u{1b}\\".to_owned()
    }
    pub fn query_osc() -> String {
        "\u{1b}]7501;?\u{1b}\\".to_owned()
    }
}

pub fn is_invisible_formatting(c: char) -> bool {
    matches!(
        c,
        '\u{061C}'
            | '\u{00AD}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
    )
}

pub fn sanitize_display_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() && !is_invisible_formatting(*c))
        .collect()
}

pub fn format_duration_ms(duration_ms: u64) -> String {
    let secs = duration_ms / 1000;
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_are_ordered_by_display_priority() {
        let mut statuses = vec![
            AttentionStatus::Idle,
            AttentionStatus::Done,
            AttentionStatus::NeedsYou,
            AttentionStatus::Working,
            AttentionStatus::NewOutput,
            AttentionStatus::Failed,
            AttentionStatus::Quiet,
        ];
        statuses.sort();
        statuses.reverse();
        assert_eq!(
            statuses,
            vec![
                AttentionStatus::NeedsYou,
                AttentionStatus::Failed,
                AttentionStatus::Done,
                AttentionStatus::Quiet,
                AttentionStatus::NewOutput,
                AttentionStatus::Working,
                AttentionStatus::Idle,
            ]
        );
    }

    #[test]
    fn command_result_from_exit() {
        assert_eq!(
            CommandResult::from_exit(Some(0), None),
            CommandResult::Succeeded
        );
        assert_eq!(
            CommandResult::from_exit(Some(1), None),
            CommandResult::Failed
        );
        assert_eq!(CommandResult::from_exit(None, None), CommandResult::Unknown);
        assert_eq!(
            CommandResult::from_exit(Some(0), Some("CANCEL")),
            CommandResult::Failed
        );
        assert_eq!(
            CommandResult::from_exit(Some(0), Some("")),
            CommandResult::Succeeded
        );
        assert_eq!(
            CommandResult::from_exit(None, Some("")),
            CommandResult::Succeeded
        );
    }

    #[test]
    fn sanitize_removes_bidi_and_controls() {
        assert_eq!(sanitize_display_text("a\u{202E}b\u{200B}c\u{7}"), "abc");
    }

    #[test]
    fn counts_exclude_muted_and_track_highest() {
        let mut counts = AttentionCounts::default();
        counts.add(&PaneAttention {
            status: AttentionStatus::Failed,
            ..Default::default()
        });
        counts.add(&PaneAttention {
            status: AttentionStatus::NeedsYou,
            muted: Some(MuteScope::Pane),
            ..Default::default()
        });
        counts.add(&PaneAttention {
            status: AttentionStatus::NewOutput,
            ..Default::default()
        });
        assert_eq!(counts.failed, 1);
        assert_eq!(counts.needs_you, 0);
        assert_eq!(counts.muted, 1);
        assert_eq!(counts.new_output, 1);
        assert_eq!(counts.highest_counted(), AttentionStatus::Failed);
    }

    fn session_with(items: Vec<(u32, AttentionStatus, u64, bool)>) -> SessionAttention {
        let mut counts = AttentionCounts::default();
        let items: Vec<AttentionItem> = items
            .into_iter()
            .map(|(id, status, since_ms, muted)| {
                let attention = PaneAttention {
                    status,
                    source: Some(AttentionSource::ShellCommand),
                    text: format!("exited {}: secret", id),
                    since_ms,
                    muted: if muted { Some(MuteScope::Pane) } else { None },
                    ..Default::default()
                };
                counts.add(&attention);
                AttentionItem {
                    pane_id: PaneId::Terminal(id),
                    tab_id: 0,
                    tab_name: "tab".to_owned(),
                    pane_title: format!("pane{}", id),
                    attention,
                }
            })
            .collect();
        SessionAttention {
            status: counts.highest_counted(),
            counts,
            items,
            ..Default::default()
        }
    }

    #[test]
    fn global_attention_merges_sorts_and_counts_every_session() {
        let global = GlobalAttention::build(
            "here",
            vec![
                (
                    "api".to_owned(),
                    session_with(vec![
                        (1, AttentionStatus::Done, 50, false),
                        (2, AttentionStatus::NeedsYou, 10, true),
                    ]),
                ),
                (
                    "here".to_owned(),
                    session_with(vec![
                        (1, AttentionStatus::NeedsYou, 20, false),
                        (3, AttentionStatus::NewOutput, 90, false),
                    ]),
                ),
                (
                    "build".to_owned(),
                    session_with(vec![(4, AttentionStatus::NeedsYou, 30, false)]),
                ),
            ],
        );
        assert_eq!(global.running(), 3);
        assert_eq!(global.sessions[0].name, "here");
        assert_eq!(global.counts.needs_you, 2);
        assert_eq!(global.counts.done, 1);
        assert_eq!(global.counts.muted, 1);
        assert_eq!(global.top(), Some((AttentionStatus::NeedsYou, 2)));
        let order: Vec<(String, PaneId)> = global
            .items
            .iter()
            .map(|i| (i.session.clone(), i.item.pane_id))
            .collect();
        assert_eq!(
            order,
            vec![
                ("build".to_owned(), PaneId::Terminal(4)),
                ("here".to_owned(), PaneId::Terminal(1)),
                ("api".to_owned(), PaneId::Terminal(1)),
                ("here".to_owned(), PaneId::Terminal(3)),
                ("api".to_owned(), PaneId::Terminal(2)),
            ]
        );
        assert_eq!(global.counted_items().len(), 3);
        assert_eq!(global.activity_items().len(), 1);
        assert_eq!(global.muted_items().len(), 1);
        assert_eq!(global.items[1].place(), "here pane1");
        assert_eq!(global.items[0].place(), "build pane4");
    }

    #[test]
    fn next_counted_item_wraps_and_skips_activity() {
        let global = GlobalAttention::build(
            "here",
            vec![
                (
                    "here".to_owned(),
                    session_with(vec![
                        (1, AttentionStatus::NeedsYou, 20, false),
                        (3, AttentionStatus::NewOutput, 90, false),
                    ]),
                ),
                (
                    "api".to_owned(),
                    session_with(vec![(2, AttentionStatus::Failed, 10, false)]),
                ),
            ],
        );
        let first = global.next_counted_after("here", None).unwrap();
        assert!(first.is_same_pane("here", &PaneId::Terminal(1)));
        let second = global
            .next_counted_after("here", Some(&PaneId::Terminal(1)))
            .unwrap();
        assert!(second.is_same_pane("api", &PaneId::Terminal(2)));
        let wrapped = global
            .next_counted_after("api", Some(&PaneId::Terminal(2)))
            .unwrap();
        assert!(wrapped.is_same_pane("here", &PaneId::Terminal(1)));
        let from_activity = global
            .next_counted_after("here", Some(&PaneId::Terminal(3)))
            .unwrap();
        assert!(from_activity.is_same_pane("here", &PaneId::Terminal(1)));
    }

    #[test]
    fn global_attention_can_drop_command_text() {
        let global = GlobalAttention::build(
            "here",
            vec![(
                "api".to_owned(),
                session_with(vec![(1, AttentionStatus::Failed, 1, false)]),
            )],
        )
        .without_command_text();
        assert_eq!(global.items[0].item.attention.text, "exited 1");
        assert_eq!(
            global.sessions[0].attention.items[0].attention.text,
            "exited 1"
        );
    }

    #[test]
    fn stripping_command_text_keeps_the_head() {
        let attention = PaneAttention {
            status: AttentionStatus::Failed,
            source: Some(AttentionSource::ShellCommand),
            text: "exited 1: cargo build".to_owned(),
            detail: AttentionDetail {
                command: Some(CommandText {
                    text: "cargo build".to_owned(),
                    source: CommandTextSource::Shell,
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let stripped = attention.without_command_text();
        assert_eq!(stripped.text, "exited 1");
        assert_eq!(stripped.detail.command, None);
    }
}

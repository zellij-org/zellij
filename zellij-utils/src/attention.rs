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
        assert_eq!(CommandResult::from_exit(Some(0), None), CommandResult::Succeeded);
        assert_eq!(CommandResult::from_exit(Some(1), None), CommandResult::Failed);
        assert_eq!(CommandResult::from_exit(None, None), CommandResult::Unknown);
        assert_eq!(
            CommandResult::from_exit(Some(0), Some("CANCEL")),
            CommandResult::Failed
        );
        assert_eq!(
            CommandResult::from_exit(Some(0), Some("")),
            CommandResult::Succeeded
        );
        assert_eq!(CommandResult::from_exit(None, Some("")), CommandResult::Succeeded);
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

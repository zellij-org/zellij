use zellij_utils::attention::{CommandText, ReportedCwd};

use super::program_status::ProgramStatusMessage;
use super::shell::ShellMark;

const MAX_PENDING_SIGNALS: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneSignal {
    ProgramStatus(ProgramStatusMessage),
    Reset,
    PromptShown {
        aid: Option<String>,
    },
    InputStarted,
    CommandStarted {
        command: Option<CommandText>,
        aid: Option<String>,
    },
    CommandFinished {
        exit_code: Option<i32>,
        err: Option<String>,
        aid: Option<String>,
    },
    Cwd(ReportedCwd),
    Bell,
}

#[derive(Debug, Clone, Default)]
pub struct ShellReportState {
    signals: Vec<PaneSignal>,
    last_mark: Option<ShellMark>,
    pub prompt_open: bool,
    pub pending_command_text: Option<CommandText>,
    pub iterm_remote_host: Option<String>,
    pub reports_markers: bool,
}

impl ShellReportState {
    pub fn push(&mut self, signal: PaneSignal) {
        if matches!(signal, PaneSignal::Bell)
            && matches!(self.signals.last(), Some(PaneSignal::Bell))
        {
            return;
        }
        if self.signals.len() >= MAX_PENDING_SIGNALS {
            self.signals.remove(0);
        }
        self.signals.push(signal);
    }
    pub fn drain(&mut self) -> Vec<PaneSignal> {
        std::mem::take(&mut self.signals)
    }
    pub fn has_signals(&self) -> bool {
        !self.signals.is_empty()
    }
    pub fn is_duplicate(&self, mark: &ShellMark) -> bool {
        self.last_mark
            .as_ref()
            .map(|last| last.same_event_as(mark))
            .unwrap_or(false)
    }
    pub fn remember(&mut self, mark: ShellMark) {
        self.last_mark = Some(mark);
    }
    pub fn reset(&mut self) {
        self.last_mark = None;
        self.prompt_open = false;
        self.pending_command_text = None;
        self.iterm_remote_host = None;
        self.push(PaneSignal::Reset);
    }
}

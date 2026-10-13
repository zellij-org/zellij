use super::generated_api::api::event::{
    AttentionCounts as ProtobufAttentionCounts, AttentionDetail as ProtobufAttentionDetail,
    CommandOutcome as ProtobufCommandOutcome, CommandState as ProtobufCommandState,
    CommandText as ProtobufCommandText, PaneAttention as ProtobufPaneAttention,
    ProgramStatusRecord as ProtobufProgramStatusRecord, ReportedCwd as ProtobufReportedCwd,
    RunningCommand as ProtobufRunningCommand, TabAttention as ProtobufTabAttention,
};
use crate::attention::{
    AttentionCounts, AttentionDetail, AttentionSource, AttentionStatus, BlockedKind,
    CommandOutcome, CommandPhase, CommandResult, CommandState, CommandText, CommandTextSource,
    CwdSource, MuteScope, PaneAttention, ProgramState, ProgramStatusRecord, ReportedCwd,
    RunningCommand, TabAttention,
};

impl From<CommandText> for ProtobufCommandText {
    fn from(command: CommandText) -> Self {
        ProtobufCommandText {
            text: command.text,
            source: command.source.as_str().to_owned(),
        }
    }
}

impl From<ProtobufCommandText> for CommandText {
    fn from(command: ProtobufCommandText) -> Self {
        CommandText {
            text: command.text,
            source: CommandTextSource::from_str(&command.source)
                .unwrap_or(CommandTextSource::Shell),
        }
    }
}

impl From<AttentionDetail> for ProtobufAttentionDetail {
    fn from(detail: AttentionDetail) -> Self {
        ProtobufAttentionDetail {
            kind: detail.kind.map(|k| k.as_str().to_owned()),
            progress: detail.progress.map(|p| p as u32),
            app: detail.app,
            title: detail.title,
            body: detail.body,
            exit_code: detail.exit_code,
            duration_ms: detail.duration_ms,
            command: detail.command.map(|c| c.into()),
        }
    }
}

impl From<ProtobufAttentionDetail> for AttentionDetail {
    fn from(detail: ProtobufAttentionDetail) -> Self {
        AttentionDetail {
            kind: detail.kind.as_deref().and_then(BlockedKind::from_str),
            progress: detail.progress.map(|p| p.min(100) as u8),
            app: detail.app,
            title: detail.title,
            body: detail.body,
            exit_code: detail.exit_code,
            duration_ms: detail.duration_ms,
            command: detail.command.map(|c| c.into()),
        }
    }
}

impl From<PaneAttention> for ProtobufPaneAttention {
    fn from(attention: PaneAttention) -> Self {
        ProtobufPaneAttention {
            status: attention.status.as_str().to_owned(),
            source: attention.source.map(|s| s.as_str().to_owned()),
            text: attention.text,
            detail: Some(attention.detail.into()),
            since_ms: attention.since_ms,
            changed_ms: attention.changed_ms,
            seen: attention.seen,
            muted: attention.muted.map(|m| m.as_str().to_owned()),
            ignore_output: attention.ignore_output,
            watched: attention.watched,
            from_watch: attention.from_watch,
        }
    }
}

impl From<ProtobufPaneAttention> for PaneAttention {
    fn from(attention: ProtobufPaneAttention) -> Self {
        PaneAttention {
            status: AttentionStatus::from_str(&attention.status).unwrap_or_default(),
            source: attention.source.as_deref().and_then(AttentionSource::from_str),
            text: attention.text,
            detail: attention.detail.map(|d| d.into()).unwrap_or_default(),
            since_ms: attention.since_ms,
            changed_ms: attention.changed_ms,
            seen: attention.seen,
            muted: attention.muted.as_deref().and_then(MuteScope::from_str),
            ignore_output: attention.ignore_output,
            watched: attention.watched,
            from_watch: attention.from_watch,
        }
    }
}

impl From<CommandOutcome> for ProtobufCommandOutcome {
    fn from(outcome: CommandOutcome) -> Self {
        ProtobufCommandOutcome {
            result: outcome.result.as_str().to_owned(),
            exit_code: outcome.exit_code,
            err: outcome.err,
            duration_ms: outcome.duration_ms,
            ended_ms: outcome.ended_ms,
            command: outcome.command.map(|c| c.into()),
        }
    }
}

impl From<ProtobufCommandOutcome> for CommandOutcome {
    fn from(outcome: ProtobufCommandOutcome) -> Self {
        CommandOutcome {
            result: CommandResult::from_str(&outcome.result).unwrap_or(CommandResult::Unknown),
            exit_code: outcome.exit_code,
            err: outcome.err,
            duration_ms: outcome.duration_ms,
            ended_ms: outcome.ended_ms,
            command: outcome.command.map(|c| c.into()),
        }
    }
}

impl From<RunningCommand> for ProtobufRunningCommand {
    fn from(running: RunningCommand) -> Self {
        ProtobufRunningCommand {
            command: running.command.map(|c| c.into()),
            started_ms: running.started_ms,
        }
    }
}

impl From<ProtobufRunningCommand> for RunningCommand {
    fn from(running: ProtobufRunningCommand) -> Self {
        RunningCommand {
            command: running.command.map(|c| c.into()),
            started_ms: running.started_ms,
        }
    }
}

impl From<ReportedCwd> for ProtobufReportedCwd {
    fn from(cwd: ReportedCwd) -> Self {
        ProtobufReportedCwd {
            path: cwd.path,
            source: cwd.source.as_str().to_owned(),
            host: cwd.host,
            is_local: cwd.is_local,
        }
    }
}

impl From<ProtobufReportedCwd> for ReportedCwd {
    fn from(cwd: ProtobufReportedCwd) -> Self {
        ReportedCwd {
            path: cwd.path,
            source: CwdSource::from_str(&cwd.source).unwrap_or(CwdSource::Poll),
            host: cwd.host,
            is_local: cwd.is_local,
        }
    }
}

impl From<CommandState> for ProtobufCommandState {
    fn from(state: CommandState) -> Self {
        ProtobufCommandState {
            phase: state.phase.as_str().to_owned(),
            running: state.running.map(|r| r.into()),
            last: state.last.map(|l| l.into()),
            cwd: state.cwd.map(|c| c.into()),
            reports_markers: state.reports_markers,
        }
    }
}

impl From<ProtobufCommandState> for CommandState {
    fn from(state: ProtobufCommandState) -> Self {
        CommandState {
            phase: CommandPhase::from_str(&state.phase).unwrap_or_default(),
            running: state.running.map(|r| r.into()),
            last: state.last.map(|l| l.into()),
            cwd: state.cwd.map(|c| c.into()),
            reports_markers: state.reports_markers,
        }
    }
}

impl From<ProgramStatusRecord> for ProtobufProgramStatusRecord {
    fn from(record: ProgramStatusRecord) -> Self {
        ProtobufProgramStatusRecord {
            id: record.id,
            state: record.state.map(|s| s.as_str().to_owned()),
            kind: record.kind.map(|k| k.as_str().to_owned()),
            progress: record.progress.map(|p| p as u32),
            app: record.app,
            title: record.title,
            msg: record.msg,
            updated_ms: record.updated_ms,
        }
    }
}

impl From<ProtobufProgramStatusRecord> for ProgramStatusRecord {
    fn from(record: ProtobufProgramStatusRecord) -> Self {
        ProgramStatusRecord {
            id: record.id,
            state: record.state.as_deref().and_then(ProgramState::from_str),
            kind: record.kind.as_deref().and_then(BlockedKind::from_str),
            progress: record.progress.map(|p| p.min(100) as u8),
            app: record.app,
            title: record.title,
            msg: record.msg,
            updated_ms: record.updated_ms,
        }
    }
}

impl From<AttentionCounts> for ProtobufAttentionCounts {
    fn from(counts: AttentionCounts) -> Self {
        ProtobufAttentionCounts {
            needs_you: counts.needs_you as u32,
            failed: counts.failed as u32,
            done: counts.done as u32,
            working: counts.working as u32,
            quiet: counts.quiet as u32,
            new_output: counts.new_output as u32,
            muted: counts.muted as u32,
        }
    }
}

impl From<ProtobufAttentionCounts> for AttentionCounts {
    fn from(counts: ProtobufAttentionCounts) -> Self {
        AttentionCounts {
            needs_you: counts.needs_you as usize,
            failed: counts.failed as usize,
            done: counts.done as usize,
            working: counts.working as usize,
            quiet: counts.quiet as usize,
            new_output: counts.new_output as usize,
            muted: counts.muted as usize,
        }
    }
}

impl From<TabAttention> for ProtobufTabAttention {
    fn from(attention: TabAttention) -> Self {
        ProtobufTabAttention {
            status: attention.status.as_str().to_owned(),
            counts: Some(attention.counts.into()),
            muted: attention.muted,
        }
    }
}

impl From<ProtobufTabAttention> for TabAttention {
    fn from(attention: ProtobufTabAttention) -> Self {
        TabAttention {
            status: AttentionStatus::from_str(&attention.status).unwrap_or_default(),
            counts: attention.counts.map(|c| c.into()).unwrap_or_default(),
            muted: attention.muted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_attention_round_trips() {
        let attention = PaneAttention {
            status: AttentionStatus::NeedsYou,
            source: Some(AttentionSource::ProgramStatus),
            text: "needs approval: edit".to_owned(),
            detail: AttentionDetail {
                kind: Some(BlockedKind::Permission),
                progress: Some(40),
                app: Some("claude".to_owned()),
                title: None,
                body: Some("edit".to_owned()),
                exit_code: Some(1),
                duration_ms: Some(1200),
                command: Some(CommandText {
                    text: "claude".to_owned(),
                    source: CommandTextSource::Screen,
                }),
            },
            since_ms: 1,
            changed_ms: 2,
            seen: true,
            muted: Some(MuteScope::Tab),
            ignore_output: true,
            watched: true,
            from_watch: false,
        };
        let protobuf: ProtobufPaneAttention = attention.clone().into();
        assert_eq!(PaneAttention::from(protobuf), attention);
    }

    #[test]
    fn command_state_round_trips() {
        let state = CommandState {
            phase: CommandPhase::Running,
            running: Some(RunningCommand {
                command: None,
                started_ms: 5,
            }),
            last: Some(CommandOutcome {
                result: CommandResult::Failed,
                exit_code: Some(2),
                err: Some("x".to_owned()),
                duration_ms: Some(3),
                ended_ms: 4,
                command: None,
            }),
            cwd: Some(ReportedCwd {
                path: "/tmp".to_owned(),
                source: CwdSource::Osc7,
                host: Some("h".to_owned()),
                is_local: false,
            }),
            reports_markers: true,
        };
        let protobuf: ProtobufCommandState = state.clone().into();
        assert_eq!(CommandState::from(protobuf), state);
    }

    #[test]
    fn records_and_tabs_round_trip() {
        let record = ProgramStatusRecord {
            id: Some("a/b".to_owned()),
            state: Some(ProgramState::Blocked),
            kind: Some(BlockedKind::Auth),
            progress: Some(7),
            app: Some("x".to_owned()),
            title: Some("t".to_owned()),
            msg: Some("m".to_owned()),
            updated_ms: 9,
        };
        let protobuf: ProtobufProgramStatusRecord = record.clone().into();
        assert_eq!(ProgramStatusRecord::from(protobuf), record);
        let tab = TabAttention {
            status: AttentionStatus::Failed,
            counts: AttentionCounts {
                needs_you: 1,
                failed: 2,
                done: 3,
                working: 4,
                quiet: 5,
                new_output: 6,
                muted: 7,
            },
            muted: true,
        };
        let protobuf: ProtobufTabAttention = tab.clone().into();
        assert_eq!(TabAttention::from(protobuf), tab);
    }
}

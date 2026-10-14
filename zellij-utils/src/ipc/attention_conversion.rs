use crate::{
    attention::{
        AttentionCounts, AttentionDetail, AttentionItem, AttentionSource, AttentionStatus,
        BlockedKind, CommandText, CommandTextSource, MuteScope, PaneAttention, SessionAttention,
    },
    client_server_contract::client_server_contract::{
        session_arrival::Arrival as ProtoArrival, AttentionArrival as ProtoAttentionArrival,
        AttentionCounts as ProtoAttentionCounts, AttentionDetail as ProtoAttentionDetail,
        AttentionItem as ProtoAttentionItem, AttentionSource as ProtoAttentionSource,
        AttentionStatus as ProtoAttentionStatus, BackArrival as ProtoBackArrival,
        BlockedKind as ProtoBlockedKind, CommandText as ProtoCommandText,
        CommandTextSource as ProtoCommandTextSource, ForwardArrival as ProtoForwardArrival,
        MuteScope as ProtoMuteScope, PaneAttention as ProtoPaneAttention,
        PaneIdWithPlugin as ProtoPaneIdWithPlugin, PreviewAnchor as ProtoPreviewAnchor,
        SessionArrival as ProtoSessionArrival, SessionAttention as ProtoSessionAttention,
        SessionHistory as ProtoSessionHistory, SessionPreview as ProtoSessionPreview,
        SessionPreviewPane as ProtoSessionPreviewPane, SessionPreviewTab as ProtoSessionPreviewTab,
        StartupCard as ProtoStartupCard, SwitchArrival as ProtoSwitchArrival,
        SwitchContext as ProtoSwitchContext, ToggleArrival as ProtoToggleArrival,
    },
    data::{PreviewAnchor, SessionPreview, SessionPreviewPane, SessionPreviewTab},
    errors::prelude::*,
    input::attention::StartupCard,
    session_history::{SessionArrival, SessionHistory, SwitchContext},
};

fn attention_status_to_proto(status: AttentionStatus) -> i32 {
    let proto = match status {
        AttentionStatus::Idle => ProtoAttentionStatus::Idle,
        AttentionStatus::Working => ProtoAttentionStatus::Working,
        AttentionStatus::NewOutput => ProtoAttentionStatus::NewOutput,
        AttentionStatus::Quiet => ProtoAttentionStatus::Quiet,
        AttentionStatus::Done => ProtoAttentionStatus::Done,
        AttentionStatus::Failed => ProtoAttentionStatus::Failed,
        AttentionStatus::NeedsYou => ProtoAttentionStatus::NeedsYou,
    };
    proto as i32
}

fn attention_status_from_proto(value: i32) -> Result<AttentionStatus> {
    Ok(
        match ProtoAttentionStatus::try_from(value)
            .map_err(|_| anyhow!("Unknown AttentionStatus: {}", value))?
        {
            ProtoAttentionStatus::Idle => AttentionStatus::Idle,
            ProtoAttentionStatus::Working => AttentionStatus::Working,
            ProtoAttentionStatus::NewOutput => AttentionStatus::NewOutput,
            ProtoAttentionStatus::Quiet => AttentionStatus::Quiet,
            ProtoAttentionStatus::Done => AttentionStatus::Done,
            ProtoAttentionStatus::Failed => AttentionStatus::Failed,
            ProtoAttentionStatus::NeedsYou => AttentionStatus::NeedsYou,
        },
    )
}

fn attention_source_to_proto(source: AttentionSource) -> i32 {
    let proto = match source {
        AttentionSource::ProgramStatus => ProtoAttentionSource::ProgramStatus,
        AttentionSource::Notification => ProtoAttentionSource::Notification,
        AttentionSource::Bell => ProtoAttentionSource::Bell,
        AttentionSource::CommandPane => ProtoAttentionSource::CommandPane,
        AttentionSource::ShellCommand => ProtoAttentionSource::ShellCommand,
        AttentionSource::ForegroundProcess => ProtoAttentionSource::ForegroundProcess,
        AttentionSource::IdleRule => ProtoAttentionSource::IdleRule,
        AttentionSource::Watch => ProtoAttentionSource::Watch,
        AttentionSource::UnseenOutput => ProtoAttentionSource::UnseenOutput,
    };
    proto as i32
}

fn attention_source_from_proto(value: i32) -> Result<AttentionSource> {
    Ok(
        match ProtoAttentionSource::try_from(value)
            .map_err(|_| anyhow!("Unknown AttentionSource: {}", value))?
        {
            ProtoAttentionSource::ProgramStatus => AttentionSource::ProgramStatus,
            ProtoAttentionSource::Notification => AttentionSource::Notification,
            ProtoAttentionSource::Bell => AttentionSource::Bell,
            ProtoAttentionSource::CommandPane => AttentionSource::CommandPane,
            ProtoAttentionSource::ShellCommand => AttentionSource::ShellCommand,
            ProtoAttentionSource::ForegroundProcess => AttentionSource::ForegroundProcess,
            ProtoAttentionSource::IdleRule => AttentionSource::IdleRule,
            ProtoAttentionSource::Watch => AttentionSource::Watch,
            ProtoAttentionSource::UnseenOutput => AttentionSource::UnseenOutput,
        },
    )
}

fn blocked_kind_to_proto(kind: BlockedKind) -> i32 {
    let proto = match kind {
        BlockedKind::Permission => ProtoBlockedKind::Permission,
        BlockedKind::Question => ProtoBlockedKind::Question,
        BlockedKind::Auth => ProtoBlockedKind::Auth,
    };
    proto as i32
}

fn blocked_kind_from_proto(value: i32) -> Result<BlockedKind> {
    Ok(
        match ProtoBlockedKind::try_from(value)
            .map_err(|_| anyhow!("Unknown BlockedKind: {}", value))?
        {
            ProtoBlockedKind::Permission => BlockedKind::Permission,
            ProtoBlockedKind::Question => BlockedKind::Question,
            ProtoBlockedKind::Auth => BlockedKind::Auth,
        },
    )
}

fn mute_scope_to_proto(scope: MuteScope) -> i32 {
    let proto = match scope {
        MuteScope::Pane => ProtoMuteScope::Pane,
        MuteScope::Tab => ProtoMuteScope::Tab,
        MuteScope::Session => ProtoMuteScope::Session,
    };
    proto as i32
}

fn mute_scope_from_proto(value: i32) -> Result<MuteScope> {
    Ok(
        match ProtoMuteScope::try_from(value)
            .map_err(|_| anyhow!("Unknown MuteScope: {}", value))?
        {
            ProtoMuteScope::Pane => MuteScope::Pane,
            ProtoMuteScope::Tab => MuteScope::Tab,
            ProtoMuteScope::Session => MuteScope::Session,
        },
    )
}

fn command_text_source_to_proto(source: CommandTextSource) -> i32 {
    let proto = match source {
        CommandTextSource::Shell => ProtoCommandTextSource::Shell,
        CommandTextSource::Screen => ProtoCommandTextSource::Screen,
        CommandTextSource::Process => ProtoCommandTextSource::Process,
    };
    proto as i32
}

fn command_text_source_from_proto(value: i32) -> Result<CommandTextSource> {
    Ok(
        match ProtoCommandTextSource::try_from(value)
            .map_err(|_| anyhow!("Unknown CommandTextSource: {}", value))?
        {
            ProtoCommandTextSource::Shell => CommandTextSource::Shell,
            ProtoCommandTextSource::Screen => CommandTextSource::Screen,
            ProtoCommandTextSource::Process => CommandTextSource::Process,
        },
    )
}

fn startup_card_to_proto(startup_card: StartupCard) -> i32 {
    let proto = match startup_card {
        StartupCard::Attention => ProtoStartupCard::Attention,
        StartupCard::Always => ProtoStartupCard::Always,
        StartupCard::Never => ProtoStartupCard::Never,
    };
    proto as i32
}

fn startup_card_from_proto(value: i32) -> Result<StartupCard> {
    Ok(
        match ProtoStartupCard::try_from(value)
            .map_err(|_| anyhow!("Unknown StartupCard: {}", value))?
        {
            ProtoStartupCard::Attention => StartupCard::Attention,
            ProtoStartupCard::Always => StartupCard::Always,
            ProtoStartupCard::Never => StartupCard::Never,
        },
    )
}

impl From<CommandText> for ProtoCommandText {
    fn from(command: CommandText) -> Self {
        ProtoCommandText {
            text: command.text,
            source: command_text_source_to_proto(command.source),
        }
    }
}

impl TryFrom<ProtoCommandText> for CommandText {
    type Error = anyhow::Error;
    fn try_from(command: ProtoCommandText) -> Result<Self> {
        Ok(CommandText {
            text: command.text,
            source: command_text_source_from_proto(command.source)?,
        })
    }
}

impl From<AttentionDetail> for ProtoAttentionDetail {
    fn from(detail: AttentionDetail) -> Self {
        ProtoAttentionDetail {
            kind: detail.kind.map(blocked_kind_to_proto),
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

impl TryFrom<ProtoAttentionDetail> for AttentionDetail {
    type Error = anyhow::Error;
    fn try_from(detail: ProtoAttentionDetail) -> Result<Self> {
        Ok(AttentionDetail {
            kind: detail.kind.map(blocked_kind_from_proto).transpose()?,
            progress: detail.progress.map(|p| p.min(100) as u8),
            app: detail.app,
            title: detail.title,
            body: detail.body,
            exit_code: detail.exit_code,
            duration_ms: detail.duration_ms,
            command: detail.command.map(|c| c.try_into()).transpose()?,
        })
    }
}

impl From<PaneAttention> for ProtoPaneAttention {
    fn from(attention: PaneAttention) -> Self {
        ProtoPaneAttention {
            status: attention_status_to_proto(attention.status),
            source: attention.source.map(attention_source_to_proto),
            text: attention.text,
            detail: Some(attention.detail.into()),
            since_ms: attention.since_ms,
            changed_ms: attention.changed_ms,
            seen: attention.seen,
            muted: attention.muted.map(mute_scope_to_proto),
            ignore_output: attention.ignore_output,
            watched: attention.watched,
            from_watch: attention.from_watch,
        }
    }
}

impl TryFrom<ProtoPaneAttention> for PaneAttention {
    type Error = anyhow::Error;
    fn try_from(attention: ProtoPaneAttention) -> Result<Self> {
        Ok(PaneAttention {
            status: attention_status_from_proto(attention.status)?,
            source: attention
                .source
                .map(attention_source_from_proto)
                .transpose()?,
            text: attention.text,
            detail: attention
                .detail
                .map(|d| d.try_into())
                .transpose()?
                .unwrap_or_default(),
            since_ms: attention.since_ms,
            changed_ms: attention.changed_ms,
            seen: attention.seen,
            muted: attention.muted.map(mute_scope_from_proto).transpose()?,
            ignore_output: attention.ignore_output,
            watched: attention.watched,
            from_watch: attention.from_watch,
        })
    }
}

impl From<AttentionCounts> for ProtoAttentionCounts {
    fn from(counts: AttentionCounts) -> Self {
        ProtoAttentionCounts {
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

impl From<ProtoAttentionCounts> for AttentionCounts {
    fn from(counts: ProtoAttentionCounts) -> Self {
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

impl From<AttentionItem> for ProtoAttentionItem {
    fn from(item: AttentionItem) -> Self {
        ProtoAttentionItem {
            pane_id: Some(item.pane_id.into()),
            tab_id: item.tab_id as u64,
            tab_name: item.tab_name,
            pane_title: item.pane_title,
            attention: Some(item.attention.into()),
        }
    }
}

impl TryFrom<ProtoAttentionItem> for AttentionItem {
    type Error = anyhow::Error;
    fn try_from(item: ProtoAttentionItem) -> Result<Self> {
        Ok(AttentionItem {
            pane_id: item
                .pane_id
                .ok_or_else(|| anyhow!("AttentionItem missing pane_id"))?
                .try_into()?,
            tab_id: item.tab_id as usize,
            tab_name: item.tab_name,
            pane_title: item.pane_title,
            attention: item
                .attention
                .map(|a| a.try_into())
                .transpose()?
                .unwrap_or_default(),
        })
    }
}

impl From<SessionAttention> for ProtoSessionAttention {
    fn from(attention: SessionAttention) -> Self {
        ProtoSessionAttention {
            status: attention_status_to_proto(attention.status),
            counts: Some(attention.counts.into()),
            muted: attention.muted,
            top: attention.top.map(|t| t.into()),
            items: attention.items.into_iter().map(|i| i.into()).collect(),
        }
    }
}

impl TryFrom<ProtoSessionAttention> for SessionAttention {
    type Error = anyhow::Error;
    fn try_from(attention: ProtoSessionAttention) -> Result<Self> {
        Ok(SessionAttention {
            status: attention_status_from_proto(attention.status)?,
            counts: attention.counts.map(|c| c.into()).unwrap_or_default(),
            muted: attention.muted,
            top: attention.top.map(|t| t.try_into()).transpose()?,
            items: attention
                .items
                .into_iter()
                .map(|i| i.try_into())
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl From<SessionHistory> for ProtoSessionHistory {
    fn from(history: SessionHistory) -> Self {
        ProtoSessionHistory {
            back: history.back,
            forward: history.forward,
            last: history.last,
        }
    }
}

impl From<ProtoSessionHistory> for SessionHistory {
    fn from(history: ProtoSessionHistory) -> Self {
        SessionHistory {
            back: history.back,
            forward: history.forward,
            last: history.last,
        }
    }
}

impl From<SessionArrival> for ProtoSessionArrival {
    fn from(arrival: SessionArrival) -> Self {
        let arrival = match arrival {
            SessionArrival::Switch => ProtoArrival::Switch(ProtoSwitchArrival {}),
            SessionArrival::Toggle => ProtoArrival::Toggle(ProtoToggleArrival {}),
            SessionArrival::Back => ProtoArrival::Back(ProtoBackArrival {}),
            SessionArrival::Forward => ProtoArrival::Forward(ProtoForwardArrival {}),
            SessionArrival::Attention { pane_id } => {
                ProtoArrival::Attention(ProtoAttentionArrival {
                    pane_id: Some(pane_id.into()),
                })
            },
        };
        ProtoSessionArrival {
            arrival: Some(arrival),
        }
    }
}

impl TryFrom<ProtoSessionArrival> for SessionArrival {
    type Error = anyhow::Error;
    fn try_from(arrival: ProtoSessionArrival) -> Result<Self> {
        Ok(
            match arrival
                .arrival
                .ok_or_else(|| anyhow!("SessionArrival missing arrival"))?
            {
                ProtoArrival::Switch(_) => SessionArrival::Switch,
                ProtoArrival::Toggle(_) => SessionArrival::Toggle,
                ProtoArrival::Back(_) => SessionArrival::Back,
                ProtoArrival::Forward(_) => SessionArrival::Forward,
                ProtoArrival::Attention(attention) => SessionArrival::Attention {
                    pane_id: attention
                        .pane_id
                        .ok_or_else(|| anyhow!("Attention arrival missing pane_id"))?
                        .try_into()?,
                },
            },
        )
    }
}

impl From<SwitchContext> for ProtoSwitchContext {
    fn from(context: SwitchContext) -> Self {
        ProtoSwitchContext {
            history: Some(context.history.into()),
            arrival: context.arrival.map(|a| a.into()),
            from_session: context.from_session,
            startup_card: context.startup_card.map(startup_card_to_proto),
        }
    }
}

impl TryFrom<ProtoSwitchContext> for SwitchContext {
    type Error = anyhow::Error;
    fn try_from(context: ProtoSwitchContext) -> Result<Self> {
        Ok(SwitchContext {
            history: context.history.map(|h| h.into()).unwrap_or_default(),
            arrival: context.arrival.map(|a| a.try_into()).transpose()?,
            from_session: context.from_session,
            startup_card: context
                .startup_card
                .map(startup_card_from_proto)
                .transpose()?,
        })
    }
}

pub fn preview_anchor_to_proto(anchor: PreviewAnchor) -> i32 {
    let proto = match anchor {
        PreviewAnchor::Bottom => ProtoPreviewAnchor::Bottom,
        PreviewAnchor::LastCommandOutput => ProtoPreviewAnchor::LastCommandOutput,
        PreviewAnchor::NewOutputStart => ProtoPreviewAnchor::NewOutputStart,
    };
    proto as i32
}

pub fn preview_anchor_from_proto(value: i32) -> Result<PreviewAnchor> {
    Ok(
        match ProtoPreviewAnchor::try_from(value)
            .map_err(|_| anyhow!("Unknown PreviewAnchor: {}", value))?
        {
            ProtoPreviewAnchor::Bottom => PreviewAnchor::Bottom,
            ProtoPreviewAnchor::LastCommandOutput => PreviewAnchor::LastCommandOutput,
            ProtoPreviewAnchor::NewOutputStart => PreviewAnchor::NewOutputStart,
        },
    )
}

impl From<SessionPreviewPane> for ProtoSessionPreviewPane {
    fn from(pane: SessionPreviewPane) -> Self {
        ProtoSessionPreviewPane {
            id: pane.id,
            is_plugin: pane.is_plugin,
            title: pane.title,
            focused: pane.focused,
            contents: pane.contents,
        }
    }
}

impl From<ProtoSessionPreviewPane> for SessionPreviewPane {
    fn from(pane: ProtoSessionPreviewPane) -> Self {
        SessionPreviewPane {
            id: pane.id,
            is_plugin: pane.is_plugin,
            title: pane.title,
            focused: pane.focused,
            contents: pane.contents,
        }
    }
}

impl From<SessionPreviewTab> for ProtoSessionPreviewTab {
    fn from(tab: SessionPreviewTab) -> Self {
        ProtoSessionPreviewTab {
            name: tab.name,
            active: tab.active,
            panes: tab.panes.into_iter().map(|p| p.into()).collect(),
        }
    }
}

impl From<ProtoSessionPreviewTab> for SessionPreviewTab {
    fn from(tab: ProtoSessionPreviewTab) -> Self {
        SessionPreviewTab {
            name: tab.name,
            active: tab.active,
            panes: tab.panes.into_iter().map(|p| p.into()).collect(),
        }
    }
}

impl From<SessionPreview> for ProtoSessionPreview {
    fn from(preview: SessionPreview) -> Self {
        ProtoSessionPreview {
            session_name: preview.session_name,
            tab_index: preview.tab_index.map(|t| t as u32),
            active_tab_index: preview.active_tab_index.map(|t| t as u32),
            tab_names: preview.tab_names,
            contents: preview.contents,
            pane_count: preview.pane_count as u32,
            connected_clients: preview.connected_clients as u32,
            error: preview.error,
            tabs: preview.tabs.into_iter().map(|t| t.into()).collect(),
            pane_id: preview
                .pane_id
                .map(|(pane_id, is_plugin)| ProtoPaneIdWithPlugin { pane_id, is_plugin }),
            anchor_line: preview.anchor_line.map(|l| l as u32),
        }
    }
}

impl From<ProtoSessionPreview> for SessionPreview {
    fn from(preview: ProtoSessionPreview) -> Self {
        SessionPreview {
            session_name: preview.session_name,
            tab_index: preview.tab_index.map(|t| t as usize),
            active_tab_index: preview.active_tab_index.map(|t| t as usize),
            tab_names: preview.tab_names,
            contents: preview.contents,
            pane_count: preview.pane_count as usize,
            connected_clients: preview.connected_clients as usize,
            error: preview.error,
            tabs: preview.tabs.into_iter().map(|t| t.into()).collect(),
            pane_id: preview.pane_id.map(|p| (p.pane_id, p.is_plugin)),
            anchor_line: preview.anchor_line.map(|l| l as usize),
        }
    }
}

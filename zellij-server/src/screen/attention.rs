use super::Screen;
use crate::attention::{now_ms, AttentionEvent};
use crate::background_jobs::BackgroundJob;
use crate::panes::PaneId;
use crate::plugins::PluginInstruction;
use crate::tab::Tab;
use crate::ClientId;
use std::collections::HashSet;
use zellij_utils::attention::{
    AttentionSource, AttentionStatus, BlockedKind, HostProgramStatus, SessionAttention,
    TabAttention,
};
use zellij_utils::data::{
    ClickedPaneAction, ClickedTabAction, ContextMenuAction, ContextMenuContext, ContextMenuEntry,
    Event, PaneInfo,
};
use zellij_utils::input::actions::Action;

impl Screen {
    fn attention_pane_ids(tab: &Tab) -> Vec<zellij_utils::data::PaneId> {
        tab.get_all_pane_ids()
            .into_iter()
            .filter(|p| matches!(p, PaneId::Terminal(_)))
            .map(|p| p.into())
            .collect()
    }

    pub(super) fn tab_attention(&self, tab: &Tab) -> TabAttention {
        self.attention
            .tab_attention(tab.id, &Self::attention_pane_ids(tab))
    }

    pub(super) fn decorate_pane_infos(&self, pane_infos: &mut Vec<PaneInfo>) {
        for pane_info in pane_infos.iter_mut() {
            if pane_info.is_plugin {
                continue;
            }
            let pane_id = zellij_utils::data::PaneId::Terminal(pane_info.id);
            if let Some(attention) = self.attention.attention(&pane_id) {
                pane_info.attention = attention.clone();
            }
            if let Some(command_state) = self.attention.command_state(&pane_id) {
                pane_info.command_state = command_state.clone();
            }
            pane_info.program_status = self.attention.program_status_records(&pane_id);
        }
    }

    fn attention_visibility(
        &self,
    ) -> (
        Vec<(zellij_utils::data::PaneId, usize)>,
        HashSet<zellij_utils::data::PaneId>,
        HashSet<zellij_utils::data::PaneId>,
    ) {
        let mut panes = vec![];
        for tab in self.tabs.values() {
            for pane_id in Self::attention_pane_ids(tab) {
                panes.push((pane_id, tab.id));
            }
        }
        let mut visible = HashSet::new();
        let mut focused = HashSet::new();
        for (client_id, tab_id) in &self.active_tab_ids {
            if self.watcher_clients.contains_key(client_id) {
                continue;
            }
            if let Some(tab) = self.tabs.get(tab_id) {
                visible.extend(
                    tab.kitty_visible_pane_ids()
                        .into_iter()
                        .map(Into::<zellij_utils::data::PaneId>::into),
                );
                if let Some(active) = tab.get_active_pane_id(*client_id) {
                    focused.insert(active.into());
                }
            }
        }
        (panes, visible, focused)
    }

    fn apply_initial_attention_flags(&mut self, now: u64) {
        let mut flags = vec![];
        for tab in self.tabs.values_mut() {
            for pane_id in tab.get_all_pane_ids() {
                if let Some(pane) = tab.get_pane_with_id_mut(pane_id) {
                    if let Some(initial) = pane.take_initial_attention_flags() {
                        flags.push((pane_id, initial));
                    }
                }
            }
            if let Some(muted) = tab.take_initial_attention_muted() {
                if muted {
                    self.attention.set_tab_muted(tab.id, true, now);
                }
            }
        }
        for (pane_id, (muted, ignore_output)) in flags {
            let pane_id: zellij_utils::data::PaneId = pane_id.into();
            if muted {
                self.attention.set_pane_muted(pane_id, true, now);
            }
            if ignore_output {
                self.attention.set_pane_ignore_output(pane_id, true, now);
            }
        }
    }

    pub(super) fn sync_attention_visibility(&mut self) {
        let now = now_ms();
        self.apply_initial_attention_flags(now);
        let (panes, visible, focused) = self.attention_visibility();
        self.attention.sync_panes(&panes, &visible, &focused, now);
        self.flush_attention(false);
    }

    pub(super) fn process_attention(&mut self) {
        let now = now_ms();
        let mut signals = vec![];
        for tab in self.tabs.values_mut() {
            signals.extend(tab.pending_pane_signals.drain(..));
        }
        for (pane_id, pane_signals) in signals {
            self.attention.on_signals(pane_id.into(), pane_signals, now);
        }
        self.flush_attention(true);
    }

    pub(super) fn attention_output(&mut self, pid: u32) {
        let pane_id = zellij_utils::data::PaneId::Terminal(pid);
        if self.attention.has_pane(&pane_id) {
            self.attention.on_output(pane_id, now_ms());
        }
    }

    fn apply_attention_markers(&mut self, changed: Vec<zellij_utils::data::PaneId>) {
        let enabled = self.attention.config().frame_markers();
        for pane_id in changed {
            let marker = self.attention.frame_marker(&pane_id);
            let server_pane_id: PaneId = pane_id.into();
            for tab in self.tabs.values_mut() {
                if let Some(pane) = tab.get_pane_with_id_mut(server_pane_id) {
                    pane.set_attention_marker(marker, enabled);
                    break;
                }
            }
        }
    }

    pub(super) fn flush_attention(&mut self, request_render: bool) {
        for (pane_id, delay_ms) in self.attention.take_idle_checks() {
            let _ = self
                .bus
                .senders
                .send_to_background_jobs(BackgroundJob::AttentionIdleCheck(
                    pane_id.into(),
                    delay_ms,
                ));
        }
        let changed: Vec<zellij_utils::data::PaneId> =
            self.attention.take_changed().into_iter().collect();
        let events = self.attention.take_events();
        let has_changes = !changed.is_empty();
        if has_changes {
            self.apply_attention_markers(changed);
        }
        if !events.is_empty() {
            self.send_attention_events(events);
        }
        if has_changes {
            let _ = self.generate_and_report_pane_state();
            let _ = self.generate_and_report_tab_state();
            self.report_session_attention();
            if request_render {
                let _ = self
                    .bus
                    .senders
                    .send_to_background_jobs(BackgroundJob::RenderToClients);
            }
        }
    }

    fn send_attention_events(&mut self, events: Vec<AttentionEvent>) {
        let updates = events
            .into_iter()
            .map(|event| {
                let event = match event {
                    AttentionEvent::StatusChanged(pane_id, attention) => {
                        Event::PaneAttentionChanged(pane_id, attention)
                    },
                    AttentionEvent::PromptShown(pane_id) => Event::PromptShown(pane_id),
                    AttentionEvent::CommandStarted(pane_id, command) => {
                        Event::CommandStarted(pane_id, command)
                    },
                    AttentionEvent::CommandFinished(pane_id, outcome) => {
                        Event::CommandFinished(pane_id, outcome)
                    },
                };
                (None, None, event)
            })
            .collect();
        let _ = self
            .bus
            .senders
            .send_to_plugin(PluginInstruction::Update(updates));
    }

    pub(super) fn session_attention(&self) -> SessionAttention {
        let mut tabs: Vec<&Tab> = self.tabs.values().collect();
        tabs.sort_by_key(|t| t.position);
        let tabs: Vec<(usize, String, Vec<(zellij_utils::data::PaneId, String)>)> = tabs
            .into_iter()
            .map(|tab| {
                let panes = Self::attention_pane_ids(tab)
                    .into_iter()
                    .map(|pane_id| {
                        let title = tab
                            .get_pane_with_id(pane_id.into())
                            .map(|p| p.current_title())
                            .unwrap_or_default();
                        (pane_id, title)
                    })
                    .collect();
                (tab.id, tab.name.clone(), panes)
            })
            .collect();
        self.attention.session_attention(&tabs)
    }

    pub(super) fn report_session_attention(&mut self) {
        let summary = self.session_attention();
        if self.last_session_attention.as_ref() == Some(&summary) {
            return;
        }
        let host_status = if self.attention.config().host_status() {
            Some(host_program_status(&summary))
        } else {
            None
        };
        self.last_session_attention = Some(summary);
        let _ = self
            .bus
            .senders
            .send_to_background_jobs(BackgroundJob::ReportHostProgramStatus(host_status));
    }

    fn attention_target_pane(
        &self,
        pane_id: Option<zellij_utils::data::PaneId>,
        client_id: ClientId,
    ) -> Option<zellij_utils::data::PaneId> {
        match pane_id {
            Some(PaneIdUtils::Terminal(id)) => Some(PaneIdUtils::Terminal(id)),
            Some(PaneIdUtils::Plugin(_)) => None,
            None => self
                .get_active_tab(client_id)
                .ok()
                .and_then(|tab| tab.get_active_pane_id(client_id))
                .filter(|p| matches!(p, PaneId::Terminal(_)))
                .map(|p| p.into()),
        }
    }

    pub(super) fn toggle_pane_mute(
        &mut self,
        pane_id: Option<zellij_utils::data::PaneId>,
        client_id: ClientId,
    ) -> bool {
        let Some(pane_id) = self.attention_target_pane(pane_id, client_id) else {
            return false;
        };
        let muted = !self.attention.pane_is_muted(&pane_id);
        self.attention.set_pane_muted(pane_id, muted, now_ms());
        self.flush_attention(true);
        true
    }

    pub(super) fn toggle_pane_ignore_output(
        &mut self,
        pane_id: Option<zellij_utils::data::PaneId>,
        client_id: ClientId,
    ) -> bool {
        let Some(pane_id) = self.attention_target_pane(pane_id, client_id) else {
            return false;
        };
        let ignore = !self.attention.pane_ignores_output(&pane_id);
        self.attention
            .set_pane_ignore_output(pane_id, ignore, now_ms());
        self.flush_attention(true);
        true
    }

    pub(super) fn toggle_pane_watch(
        &mut self,
        pane_id: Option<zellij_utils::data::PaneId>,
        client_id: ClientId,
    ) -> bool {
        let Some(pane_id) = self.attention_target_pane(pane_id, client_id) else {
            return false;
        };
        let watched = !self.attention.pane_is_watched(&pane_id);
        self.attention.set_pane_watched(pane_id, watched, now_ms());
        self.flush_attention(true);
        true
    }

    pub(super) fn arm_pane_watch(&mut self, pane_id: zellij_utils::data::PaneId) {
        self.attention.add_pane(pane_id, now_ms());
        self.attention.set_pane_watched(pane_id, true, now_ms());
        self.flush_attention(true);
    }

    pub(super) fn toggle_tab_mute(&mut self, tab_id: Option<usize>, client_id: ClientId) -> bool {
        let tab_id = match tab_id {
            Some(tab_id) => self.tabs.get(&tab_id).map(|t| t.id),
            None => self.get_active_tab(client_id).ok().map(|t| t.id),
        };
        let Some(tab_id) = tab_id else {
            return false;
        };
        let muted = !self.attention.tab_is_muted(tab_id);
        self.attention.set_tab_muted(tab_id, muted, now_ms());
        self.flush_attention(true);
        true
    }

    pub(super) fn toggle_session_mute(&mut self) {
        let muted = !self.attention.session_is_muted();
        self.attention.set_session_muted(muted, now_ms());
        self.flush_attention(true);
    }

    pub(super) fn attention_idle_check(&mut self, pane_id: zellij_utils::data::PaneId) {
        self.attention.idle_check(pane_id, now_ms());
        self.flush_attention(true);
    }

    pub(super) fn attention_foreground_changed(
        &mut self,
        pane_id: zellij_utils::data::PaneId,
        command: Option<String>,
    ) {
        if !self.attention.has_pane(&pane_id) {
            return;
        }
        self.attention.foreground_changed(pane_id, command, now_ms());
        self.flush_attention(true);
    }

    pub(super) fn attention_notification(&mut self, pid: u32, title: String, body: String) {
        let pane_id = zellij_utils::data::PaneId::Terminal(pid);
        self.attention.on_notification(pane_id, title, body, now_ms());
        self.flush_attention(true);
    }

    pub(super) fn attention_command_pane_started(&mut self, pid: u32, command: Option<String>) {
        let pane_id = zellij_utils::data::PaneId::Terminal(pid);
        self.attention.command_pane_started(pane_id, command, now_ms());
        self.flush_attention(true);
    }

    pub(super) fn attention_pane_exited(&mut self, pane_id: PaneId, exit_status: Option<i32>) {
        if let PaneId::Terminal(_) = pane_id {
            self.attention
                .pane_exited(pane_id.into(), exit_status, now_ms());
            self.flush_attention(true);
        }
    }

    pub(super) fn reconfigure_attention(
        &mut self,
        config: zellij_utils::input::attention::AttentionConfig,
    ) {
        if self.attention.config() == &config {
            return;
        }
        let host_status_changed = self.attention.config().host_status() != config.host_status();
        self.attention.set_config(config, now_ms());
        let all: Vec<zellij_utils::data::PaneId> = self
            .tabs
            .values()
            .flat_map(|t| Self::attention_pane_ids(t))
            .collect();
        self.apply_attention_markers(all);
        if host_status_changed {
            self.last_session_attention = None;
        }
        self.flush_attention(true);
        self.report_session_attention();
    }

    fn context_menu_toggle_is_on(
        &self,
        actions: &[ContextMenuAction],
        context: &ContextMenuContext,
    ) -> bool {
        let clicked_pane = context
            .target_pane_id()
            .filter(|p| matches!(p, PaneIdUtils::Terminal(_)));
        let clicked_tab = context.tab_id;
        let focused_pane = self
            .get_active_tab(context.client_id)
            .ok()
            .and_then(|tab| tab.get_active_pane_id(context.client_id))
            .map(|p| p.into());
        let active_tab = self
            .get_active_tab(context.client_id)
            .ok()
            .map(|tab| tab.id);
        actions.iter().any(|action| match action {
            ContextMenuAction::ClickedPane(ClickedPaneAction::ToggleMute) => clicked_pane
                .map(|p| self.attention.pane_is_muted(&p))
                .unwrap_or(false),
            ContextMenuAction::ClickedPane(ClickedPaneAction::ToggleIgnoreOutput) => clicked_pane
                .map(|p| self.attention.pane_ignores_output(&p))
                .unwrap_or(false),
            ContextMenuAction::ClickedPane(ClickedPaneAction::ToggleWatch) => clicked_pane
                .map(|p| self.attention.pane_is_watched(&p))
                .unwrap_or(false),
            ContextMenuAction::ClickedTab(ClickedTabAction::Mute) => clicked_tab
                .map(|t| self.attention.tab_is_muted(t))
                .unwrap_or(false),
            ContextMenuAction::Action(Action::MutePane) => focused_pane
                .map(|p| self.attention.pane_is_muted(&p))
                .unwrap_or(false),
            ContextMenuAction::Action(Action::MutePaneByPaneId { pane_id }) => {
                self.attention.pane_is_muted(pane_id)
            },
            ContextMenuAction::Action(Action::IgnorePaneOutput) => focused_pane
                .map(|p| self.attention.pane_ignores_output(&p))
                .unwrap_or(false),
            ContextMenuAction::Action(Action::IgnorePaneOutputByPaneId { pane_id }) => {
                self.attention.pane_ignores_output(pane_id)
            },
            ContextMenuAction::Action(Action::WatchPane) => focused_pane
                .map(|p| self.attention.pane_is_watched(&p))
                .unwrap_or(false),
            ContextMenuAction::Action(Action::WatchPaneByPaneId { pane_id }) => {
                self.attention.pane_is_watched(pane_id)
            },
            ContextMenuAction::Action(Action::MuteTab) => active_tab
                .map(|t| self.attention.tab_is_muted(t))
                .unwrap_or(false),
            ContextMenuAction::Action(Action::MuteTabByTabId { id }) => {
                self.attention.tab_is_muted(*id as usize)
            },
            ContextMenuAction::Action(Action::MuteSession) => self.attention.session_is_muted(),
            _ => false,
        })
    }

    pub(super) fn resolve_toggled_context_menu_labels(
        &self,
        entries: Vec<ContextMenuEntry>,
        context: &ContextMenuContext,
    ) -> Vec<ContextMenuEntry> {
        entries
            .into_iter()
            .map(|entry| match entry {
                ContextMenuEntry::Item {
                    label,
                    actions,
                    toggled_label: Some(toggled_label),
                } => {
                    let label = if self.context_menu_toggle_is_on(&actions, context) {
                        toggled_label
                    } else {
                        label
                    };
                    ContextMenuEntry::Item {
                        label,
                        actions,
                        toggled_label: None,
                    }
                },
                other => other,
            })
            .collect()
    }
}

use zellij_utils::data::PaneId as PaneIdUtils;

fn idle_host_status() -> HostProgramStatus {
    HostProgramStatus {
        state: "idle".to_owned(),
        kind: None,
        progress: None,
        msg: String::new(),
    }
}

pub fn host_program_status(summary: &SessionAttention) -> HostProgramStatus {
    let Some(top) = summary.items.iter().find(|item| {
        !item.attention.is_muted()
            && item.attention.status.is_counted()
            && (item.attention.status != AttentionStatus::Working
                || matches!(
                    item.attention.source,
                    Some(AttentionSource::ProgramStatus) | Some(AttentionSource::CommandPane)
                ))
    }) else {
        return idle_host_status();
    };
    let state = match top.attention.status {
        AttentionStatus::NeedsYou => "blocked",
        AttentionStatus::Failed => "error",
        AttentionStatus::Done => "done",
        AttentionStatus::Working => "working",
        _ => return idle_host_status(),
    };
    let kind = if top.attention.status == AttentionStatus::NeedsYou {
        top.attention
            .detail
            .kind
            .map(|k: BlockedKind| k.as_str().to_owned())
    } else {
        None
    };
    let progress = if matches!(
        top.attention.status,
        AttentionStatus::NeedsYou | AttentionStatus::Working
    ) {
        top.attention.detail.progress
    } else {
        None
    };
    let mut item = top.clone();
    item.attention = item.attention.without_command_text();
    HostProgramStatus {
        state: state.to_owned(),
        kind,
        progress,
        msg: item.label(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_utils::attention::{AttentionItem, MuteScope, PaneAttention};

    fn item(status: AttentionStatus, source: AttentionSource, text: &str) -> AttentionItem {
        AttentionItem {
            pane_id: PaneIdUtils::Terminal(1),
            tab_id: 0,
            tab_name: "api".to_owned(),
            pane_title: "claude".to_owned(),
            attention: PaneAttention {
                status,
                source: Some(source),
                text: text.to_owned(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn nothing_to_report_is_idle() {
        let status = host_program_status(&SessionAttention::default());
        assert_eq!(status.state, "idle");
        assert_eq!(status.msg, "");
    }

    #[test]
    fn the_highest_item_is_reported_without_command_text() {
        let summary = SessionAttention {
            items: vec![item(
                AttentionStatus::Failed,
                AttentionSource::ShellCommand,
                "exited 1: cargo build",
            )],
            ..Default::default()
        };
        let status = host_program_status(&summary);
        assert_eq!(status.state, "error");
        assert_eq!(status.msg, "api › claude: exited 1");
    }

    #[test]
    fn shell_commands_running_and_muted_items_are_not_reported() {
        let mut muted = item(
            AttentionStatus::NeedsYou,
            AttentionSource::ProgramStatus,
            "needs approval",
        );
        muted.attention.muted = Some(MuteScope::Pane);
        let summary = SessionAttention {
            items: vec![
                muted,
                item(
                    AttentionStatus::Working,
                    AttentionSource::ShellCommand,
                    "running: vim",
                ),
            ],
            ..Default::default()
        };
        assert_eq!(host_program_status(&summary).state, "idle");
    }

    #[test]
    fn blocked_programs_report_their_kind() {
        let mut blocked = item(
            AttentionStatus::NeedsYou,
            AttentionSource::ProgramStatus,
            "needs approval: Allow edit?",
        );
        blocked.attention.detail.kind = Some(BlockedKind::Permission);
        let summary = SessionAttention {
            items: vec![blocked],
            ..Default::default()
        };
        let status = host_program_status(&summary);
        assert_eq!(status.state, "blocked");
        assert_eq!(status.kind.as_deref(), Some("permission"));
        assert_eq!(status.msg, "api › claude: needs approval: Allow edit?");
    }
}

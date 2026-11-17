use super::Screen;
use crate::ClientId;
use std::collections::{BTreeMap, HashMap};
use zellij_utils::data::{PaneId, PaneInfo, PaneManifest};
use zellij_utils::ipc::{ExitReason, ServerToClientMsg};
use zellij_utils::session_events::{
    input_mode_name, CliClientState, CliEvent, CliEventFilter, CliPaneState, CliTabState,
};

#[derive(Debug, Default)]
pub(crate) struct EventSubscribers {
    filters: HashMap<ClientId, CliEventFilter>,
    previous: Option<SessionSnapshot>,
}

impl EventSubscribers {
    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }

    pub fn remove(&mut self, client_id: &ClientId) {
        self.filters.remove(client_id);
        if self.filters.is_empty() {
            self.previous = None;
        }
    }

    #[cfg(test)]
    pub fn contains(&self, client_id: &ClientId) -> bool {
        self.filters.contains_key(client_id)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SessionSnapshot {
    pub session_name: String,
    pub tabs: Vec<CliTabState>,
    pub panes: Vec<CliPaneState>,
    pub clients: Vec<CliClientState>,
}

impl SessionSnapshot {
    fn to_initial_state(&self) -> CliEvent {
        CliEvent::InitialState {
            session_name: self.session_name.clone(),
            tabs: self.tabs.clone(),
            panes: self.panes.clone(),
            clients: self.clients.clone(),
        }
    }
}

fn pane_state(pane_info: PaneInfo, tab_id: usize) -> CliPaneState {
    let pane_id = if pane_info.is_plugin {
        PaneId::Plugin(pane_info.id)
    } else {
        PaneId::Terminal(pane_info.id)
    };
    CliPaneState {
        pane_id,
        tab_id,
        title: pane_info.title,
        is_plugin: pane_info.is_plugin,
        is_floating: pane_info.is_floating,
        is_fullscreen: pane_info.is_fullscreen,
        is_suppressed: pane_info.is_suppressed,
        is_selectable: pane_info.is_selectable,
        exited: pane_info.exited,
        exit_status: pane_info.exit_status,
        terminal_command: pane_info.terminal_command,
        plugin_url: pane_info.plugin_url,
    }
}

fn common_order(tabs: &[CliTabState], common: &dyn Fn(usize) -> bool) -> HashMap<usize, usize> {
    let mut ordered: Vec<&CliTabState> = tabs.iter().filter(|t| common(t.tab_id)).collect();
    ordered.sort_by_key(|t| t.position);
    ordered
        .iter()
        .enumerate()
        .map(|(rank, t)| (t.tab_id, rank))
        .collect()
}

pub(crate) fn diff_snapshots(prev: &SessionSnapshot, next: &SessionSnapshot) -> Vec<CliEvent> {
    let mut events = vec![];

    if prev.session_name != next.session_name {
        events.push(CliEvent::SessionRenamed {
            name: next.session_name.clone(),
            previous_name: prev.session_name.clone(),
        });
    }

    let prev_tabs: BTreeMap<usize, &CliTabState> =
        prev.tabs.iter().map(|t| (t.tab_id, t)).collect();
    let next_tabs: BTreeMap<usize, &CliTabState> =
        next.tabs.iter().map(|t| (t.tab_id, t)).collect();
    let prev_panes: HashMap<PaneId, &CliPaneState> =
        prev.panes.iter().map(|p| (p.pane_id, p)).collect();
    let next_panes: HashMap<PaneId, &CliPaneState> =
        next.panes.iter().map(|p| (p.pane_id, p)).collect();
    let prev_clients: BTreeMap<ClientId, &CliClientState> =
        prev.clients.iter().map(|c| (c.client_id, c)).collect();
    let next_clients: BTreeMap<ClientId, &CliClientState> =
        next.clients.iter().map(|c| (c.client_id, c)).collect();

    for tab in &next.tabs {
        if !prev_tabs.contains_key(&tab.tab_id) {
            events.push(CliEvent::TabCreated { tab: tab.clone() });
        }
    }

    for pane in &next.panes {
        match prev_panes.get(&pane.pane_id) {
            None => events.push(CliEvent::PaneOpened { pane: pane.clone() }),
            Some(previous) => {
                if previous.tab_id != pane.tab_id {
                    events.push(CliEvent::PaneMoved {
                        pane_id: pane.pane_id,
                        tab_id: pane.tab_id,
                        previous_tab_id: previous.tab_id,
                    });
                }
                if previous.title != pane.title {
                    events.push(CliEvent::PaneRenamed {
                        pane_id: pane.pane_id,
                        tab_id: pane.tab_id,
                        title: pane.title.clone(),
                        previous_title: previous.title.clone(),
                    });
                }
                if previous.is_floating != pane.is_floating {
                    events.push(CliEvent::PaneFloatingChanged {
                        pane_id: pane.pane_id,
                        tab_id: pane.tab_id,
                        is_floating: pane.is_floating,
                    });
                }
                if previous.is_fullscreen != pane.is_fullscreen {
                    events.push(CliEvent::PaneFullscreenChanged {
                        pane_id: pane.pane_id,
                        tab_id: pane.tab_id,
                        is_fullscreen: pane.is_fullscreen,
                    });
                }
                if pane.exited && (!previous.exited || previous.exit_status != pane.exit_status) {
                    events.push(CliEvent::PaneExited {
                        pane_id: pane.pane_id,
                        tab_id: pane.tab_id,
                        exit_status: pane.exit_status,
                    });
                }
            },
        }
    }

    for pane in &prev.panes {
        if !next_panes.contains_key(&pane.pane_id) {
            events.push(CliEvent::PaneClosed {
                pane_id: pane.pane_id,
                tab_id: Some(pane.tab_id),
            });
        }
    }

    for tab in &prev.tabs {
        if !next_tabs.contains_key(&tab.tab_id) {
            events.push(CliEvent::TabClosed {
                tab_id: tab.tab_id,
                name: tab.name.clone(),
            });
        }
    }

    let is_common =
        |tab_id: usize| prev_tabs.contains_key(&tab_id) && next_tabs.contains_key(&tab_id);
    let prev_order = common_order(&prev.tabs, &is_common);
    let next_order = common_order(&next.tabs, &is_common);
    for tab in &next.tabs {
        if let Some(previous) = prev_tabs.get(&tab.tab_id) {
            if previous.name != tab.name {
                events.push(CliEvent::TabRenamed {
                    tab_id: tab.tab_id,
                    name: tab.name.clone(),
                    previous_name: previous.name.clone(),
                });
            }
            if prev_order.get(&tab.tab_id) != next_order.get(&tab.tab_id) {
                events.push(CliEvent::TabMoved {
                    tab_id: tab.tab_id,
                    position: tab.position,
                    previous_position: previous.position,
                });
            }
        }
    }

    for client in &next.clients {
        let previous = prev_clients.get(&client.client_id);
        if previous.is_none() {
            events.push(CliEvent::ClientAttached {
                client_id: client.client_id,
                is_web_client: client.is_web_client,
            });
        }
        let previous_tab_id = previous.and_then(|c| c.tab_id);
        if let Some(tab_id) = client.tab_id {
            if Some(tab_id) != previous_tab_id {
                events.push(CliEvent::TabFocused {
                    client_id: client.client_id,
                    tab_id,
                    previous_tab_id,
                });
            }
        }
        let previous_pane_id = previous.and_then(|c| c.pane_id);
        if let (Some(pane_id), Some(tab_id)) = (client.pane_id, client.tab_id) {
            if Some(pane_id) != previous_pane_id {
                events.push(CliEvent::PaneFocused {
                    client_id: client.client_id,
                    pane_id,
                    tab_id,
                    previous_pane_id,
                });
            }
        }
        let previous_mode = previous.and_then(|c| c.mode.clone());
        if let Some(mode) = &client.mode {
            if Some(mode) != previous_mode.as_ref() {
                events.push(CliEvent::ModeChanged {
                    client_id: client.client_id,
                    mode: mode.clone(),
                    previous_mode,
                });
            }
        }
    }

    for client in &prev.clients {
        if !next_clients.contains_key(&client.client_id) {
            events.push(CliEvent::ClientDetached {
                client_id: client.client_id,
            });
        }
    }

    events
}

impl Screen {
    pub(crate) fn session_snapshot(&self, pane_manifest: Option<&PaneManifest>) -> SessionSnapshot {
        let mut tabs: Vec<CliTabState> = self
            .tabs
            .values()
            .map(|tab| CliTabState {
                tab_id: tab.id,
                position: tab.position,
                name: tab.name.clone(),
                is_fullscreen_active: tab.is_fullscreen_active(),
                is_sync_panes_active: tab.is_sync_panes_active(),
                are_floating_panes_visible: tab.are_floating_panes_visible(),
            })
            .collect();
        tabs.sort_by_key(|t| t.position);

        let mut panes: Vec<(usize, CliPaneState)> = vec![];
        for tab in self.tabs.values() {
            let pane_infos = match pane_manifest {
                Some(pane_manifest) => pane_manifest
                    .panes
                    .get(&tab.position)
                    .cloned()
                    .unwrap_or_default(),
                None => tab.pane_infos(),
            };
            for pane_info in pane_infos {
                panes.push((tab.position, pane_state(pane_info, tab.id)));
            }
        }
        panes.sort_by_key(|(tab_position, p)| {
            (
                *tab_position,
                p.is_plugin,
                match p.pane_id {
                    PaneId::Terminal(id) | PaneId::Plugin(id) => id,
                },
            )
        });
        let panes = panes.into_iter().map(|(_, p)| p).collect();

        let mut clients: Vec<CliClientState> = self
            .connected_clients
            .borrow()
            .iter()
            .map(|(client_id, is_web_client)| CliClientState {
                client_id: *client_id,
                is_web_client: *is_web_client,
                tab_id: self.active_tab_ids.get(client_id).copied(),
                pane_id: self.get_active_pane_id(client_id).map(|p| p.into()),
                mode: Some(input_mode_name(self.current_mode_for_client(*client_id).0)),
            })
            .collect();
        clients.sort_by_key(|c| c.client_id);

        SessionSnapshot {
            session_name: self.session_name.clone(),
            tabs,
            panes,
            clients,
        }
    }

    pub(crate) fn subscribe_to_events(&mut self, client_id: ClientId, filter: CliEventFilter) {
        let snapshot = self.session_snapshot(None);
        if let Some(previous) = self.event_subscribers.previous.take() {
            let events = diff_snapshots(&previous, &snapshot);
            self.deliver_session_events(&events);
        }
        let send_initial_state = filter.initial_state;
        let initial_state = filter.scope_initial_state(snapshot.to_initial_state());
        self.event_subscribers.filters.insert(client_id, filter);
        self.event_subscribers.previous = Some(snapshot);
        if send_initial_state {
            self.send_session_events_to(client_id, &[initial_state]);
        }
    }

    pub(crate) fn report_to_event_subscribers(&mut self, pane_manifest: &PaneManifest) {
        if self.event_subscribers.is_empty() {
            return;
        }
        let snapshot = self.session_snapshot(Some(pane_manifest));
        let events = match &self.event_subscribers.previous {
            Some(previous) if *previous == snapshot => return,
            Some(previous) => diff_snapshots(previous, &snapshot),
            None => vec![],
        };
        self.event_subscribers.previous = Some(snapshot);
        self.deliver_session_events(&events);
    }

    pub(crate) fn notify_event_subscribers(&mut self, event: CliEvent) {
        if self.event_subscribers.is_empty() {
            return;
        }
        let event = self.with_tab_id_of_pane(event);
        self.deliver_session_events(&[event]);
    }

    pub(crate) fn report_mode_to_event_subscribers(&mut self, client_id: ClientId) {
        if self.event_subscribers.is_empty() {
            return;
        }
        let mode = input_mode_name(self.current_mode_for_client(client_id).0);
        let Some(client) = self
            .event_subscribers
            .previous
            .as_mut()
            .and_then(|s| s.clients.iter_mut().find(|c| c.client_id == client_id))
        else {
            return;
        };
        if client.mode.as_ref() == Some(&mode) {
            return;
        }
        let previous_mode = client.mode.replace(mode.clone());
        self.deliver_session_events(&[CliEvent::ModeChanged {
            client_id,
            mode,
            previous_mode,
        }]);
    }

    pub(crate) fn end_event_subscriptions(&mut self) {
        if self.event_subscribers.is_empty() {
            return;
        }
        let snapshot = self.session_snapshot(None);
        if let Some(previous) = self.event_subscribers.previous.take() {
            let events = diff_snapshots(&previous, &snapshot);
            self.deliver_session_events(&events);
        }
        self.deliver_session_events(&[CliEvent::SessionEnding]);
        let subscriber_ids: Vec<ClientId> =
            self.event_subscribers.filters.keys().copied().collect();
        if let Some(os_input) = &self.bus.os_input {
            for subscriber_id in &subscriber_ids {
                let _ = os_input.send_to_client(
                    *subscriber_id,
                    ServerToClientMsg::Exit {
                        exit_reason: ExitReason::Normal,
                    },
                );
            }
        }
        self.event_subscribers = EventSubscribers::default();
    }

    fn with_tab_id_of_pane(&self, event: CliEvent) -> CliEvent {
        let find_tab = |pane_id: &PaneId| {
            self.event_subscribers.previous.as_ref().and_then(|s| {
                s.panes
                    .iter()
                    .find(|p| p.pane_id == *pane_id)
                    .map(|p| p.tab_id)
            })
        };
        match event {
            CliEvent::PaneCwdChanged {
                pane_id,
                tab_id: None,
                cwd,
            } => CliEvent::PaneCwdChanged {
                tab_id: find_tab(&pane_id),
                pane_id,
                cwd,
            },
            CliEvent::PaneCommandChanged {
                pane_id,
                tab_id: None,
                command,
                is_foreground,
            } => CliEvent::PaneCommandChanged {
                tab_id: find_tab(&pane_id),
                pane_id,
                command,
                is_foreground,
            },
            other => other,
        }
    }

    fn deliver_session_events(&mut self, events: &[CliEvent]) {
        if events.is_empty() {
            return;
        }
        let subscriber_ids: Vec<ClientId> =
            self.event_subscribers.filters.keys().copied().collect();
        for subscriber_id in subscriber_ids {
            self.send_session_events_to(subscriber_id, events);
        }
    }

    fn send_session_events_to(&mut self, subscriber_id: ClientId, events: &[CliEvent]) {
        let Some(filter) = self.event_subscribers.filters.get(&subscriber_id) else {
            return;
        };
        let Some(os_input) = &self.bus.os_input else {
            return;
        };
        let mut failed = false;
        for event in events.iter().filter(|e| filter.accepts(e)) {
            let json = match serde_json::to_string(event) {
                Ok(json) => json,
                Err(e) => {
                    log::error!("Failed to serialize session event: {}", e);
                    continue;
                },
            };
            if os_input
                .send_to_client(subscriber_id, ServerToClientMsg::SessionEvent { json })
                .is_err()
            {
                failed = true;
                break;
            }
        }
        if failed {
            self.event_subscribers.remove(&subscriber_id);
        }
    }
}

#[cfg(test)]
#[path = "./unit/event_subscribers_tests.rs"]
mod event_subscribers_tests;

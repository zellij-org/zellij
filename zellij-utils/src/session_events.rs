use crate::data::{ClientId, InputMode, PaneId};
use clap::ValueEnum;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
#[value(rename_all = "snake_case")]
pub enum CliEventType {
    #[value(skip)]
    InitialState,
    PaneOpened,
    PaneClosed,
    PaneFocused,
    PaneRenamed,
    PaneMoved,
    PaneFloatingChanged,
    PaneFullscreenChanged,
    PaneExited,
    PaneCwdChanged,
    PaneCommandChanged,
    TabCreated,
    TabClosed,
    TabRenamed,
    TabMoved,
    TabFocused,
    ModeChanged,
    ClientAttached,
    ClientDetached,
    SessionRenamed,
    SessionEnding,
}

impl CliEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CliEventType::InitialState => "initial_state",
            CliEventType::PaneOpened => "pane_opened",
            CliEventType::PaneClosed => "pane_closed",
            CliEventType::PaneFocused => "pane_focused",
            CliEventType::PaneRenamed => "pane_renamed",
            CliEventType::PaneMoved => "pane_moved",
            CliEventType::PaneFloatingChanged => "pane_floating_changed",
            CliEventType::PaneFullscreenChanged => "pane_fullscreen_changed",
            CliEventType::PaneExited => "pane_exited",
            CliEventType::PaneCwdChanged => "pane_cwd_changed",
            CliEventType::PaneCommandChanged => "pane_command_changed",
            CliEventType::TabCreated => "tab_created",
            CliEventType::TabClosed => "tab_closed",
            CliEventType::TabRenamed => "tab_renamed",
            CliEventType::TabMoved => "tab_moved",
            CliEventType::TabFocused => "tab_focused",
            CliEventType::ModeChanged => "mode_changed",
            CliEventType::ClientAttached => "client_attached",
            CliEventType::ClientDetached => "client_detached",
            CliEventType::SessionRenamed => "session_renamed",
            CliEventType::SessionEnding => "session_ending",
        }
    }
}

impl FromStr for CliEventType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        std::iter::once(&CliEventType::InitialState)
            .chain(CliEventType::value_variants().iter())
            .find(|v| v.as_str() == s)
            .copied()
            .ok_or_else(|| format!("Unknown event type: {}", s))
    }
}

impl std::fmt::Display for CliEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

pub fn input_mode_name(mode: InputMode) -> String {
    format!("{:?}", mode).to_lowercase()
}

mod pane_id_string {
    use super::*;

    pub fn serialize<S: Serializer>(pane_id: &PaneId, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&pane_id.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PaneId, D::Error> {
        let s = String::deserialize(deserializer)?;
        PaneId::from_str(&s).map_err(|e| serde::de::Error::custom(e.to_string()))
    }
}

mod optional_pane_id_string {
    use super::*;

    pub fn serialize<S: Serializer>(
        pane_id: &Option<PaneId>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match pane_id {
            Some(pane_id) => serializer.serialize_some(&pane_id.to_string()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<PaneId>, D::Error> {
        let s = Option::<String>::deserialize(deserializer)?;
        s.map(|s| PaneId::from_str(&s).map_err(|e| serde::de::Error::custom(e.to_string())))
            .transpose()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CliPaneState {
    #[serde(with = "pane_id_string")]
    pub pane_id: PaneId,
    pub tab_id: usize,
    pub title: String,
    pub is_plugin: bool,
    pub is_floating: bool,
    pub is_fullscreen: bool,
    pub is_suppressed: bool,
    pub is_selectable: bool,
    pub exited: bool,
    pub exit_status: Option<i32>,
    pub terminal_command: Option<String>,
    pub plugin_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CliTabState {
    pub tab_id: usize,
    pub position: usize,
    pub name: String,
    pub is_fullscreen_active: bool,
    pub is_sync_panes_active: bool,
    pub are_floating_panes_visible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CliClientState {
    pub client_id: ClientId,
    pub is_web_client: bool,
    pub tab_id: Option<usize>,
    #[serde(with = "optional_pane_id_string")]
    pub pane_id: Option<PaneId>,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CliEvent {
    InitialState {
        session_name: String,
        tabs: Vec<CliTabState>,
        panes: Vec<CliPaneState>,
        clients: Vec<CliClientState>,
    },
    PaneOpened {
        pane: CliPaneState,
    },
    PaneClosed {
        #[serde(with = "pane_id_string")]
        pane_id: PaneId,
        tab_id: Option<usize>,
    },
    PaneFocused {
        client_id: ClientId,
        #[serde(with = "pane_id_string")]
        pane_id: PaneId,
        tab_id: usize,
        #[serde(with = "optional_pane_id_string")]
        previous_pane_id: Option<PaneId>,
    },
    PaneRenamed {
        #[serde(with = "pane_id_string")]
        pane_id: PaneId,
        tab_id: usize,
        title: String,
        previous_title: String,
    },
    PaneMoved {
        #[serde(with = "pane_id_string")]
        pane_id: PaneId,
        tab_id: usize,
        previous_tab_id: usize,
    },
    PaneFloatingChanged {
        #[serde(with = "pane_id_string")]
        pane_id: PaneId,
        tab_id: usize,
        is_floating: bool,
    },
    PaneFullscreenChanged {
        #[serde(with = "pane_id_string")]
        pane_id: PaneId,
        tab_id: usize,
        is_fullscreen: bool,
    },
    PaneExited {
        #[serde(with = "pane_id_string")]
        pane_id: PaneId,
        tab_id: usize,
        exit_status: Option<i32>,
    },
    PaneCwdChanged {
        #[serde(with = "pane_id_string")]
        pane_id: PaneId,
        tab_id: Option<usize>,
        cwd: String,
    },
    PaneCommandChanged {
        #[serde(with = "pane_id_string")]
        pane_id: PaneId,
        tab_id: Option<usize>,
        command: Vec<String>,
        is_foreground: bool,
    },
    TabCreated {
        tab: CliTabState,
    },
    TabClosed {
        tab_id: usize,
        name: String,
    },
    TabRenamed {
        tab_id: usize,
        name: String,
        previous_name: String,
    },
    TabMoved {
        tab_id: usize,
        position: usize,
        previous_position: usize,
    },
    TabFocused {
        client_id: ClientId,
        tab_id: usize,
        previous_tab_id: Option<usize>,
    },
    ModeChanged {
        client_id: ClientId,
        mode: String,
        previous_mode: Option<String>,
    },
    ClientAttached {
        client_id: ClientId,
        is_web_client: bool,
    },
    ClientDetached {
        client_id: ClientId,
    },
    SessionRenamed {
        name: String,
        previous_name: String,
    },
    SessionEnding,
}

impl CliEvent {
    pub fn event_type(&self) -> CliEventType {
        match self {
            CliEvent::InitialState { .. } => CliEventType::InitialState,
            CliEvent::PaneOpened { .. } => CliEventType::PaneOpened,
            CliEvent::PaneClosed { .. } => CliEventType::PaneClosed,
            CliEvent::PaneFocused { .. } => CliEventType::PaneFocused,
            CliEvent::PaneRenamed { .. } => CliEventType::PaneRenamed,
            CliEvent::PaneMoved { .. } => CliEventType::PaneMoved,
            CliEvent::PaneFloatingChanged { .. } => CliEventType::PaneFloatingChanged,
            CliEvent::PaneFullscreenChanged { .. } => CliEventType::PaneFullscreenChanged,
            CliEvent::PaneExited { .. } => CliEventType::PaneExited,
            CliEvent::PaneCwdChanged { .. } => CliEventType::PaneCwdChanged,
            CliEvent::PaneCommandChanged { .. } => CliEventType::PaneCommandChanged,
            CliEvent::TabCreated { .. } => CliEventType::TabCreated,
            CliEvent::TabClosed { .. } => CliEventType::TabClosed,
            CliEvent::TabRenamed { .. } => CliEventType::TabRenamed,
            CliEvent::TabMoved { .. } => CliEventType::TabMoved,
            CliEvent::TabFocused { .. } => CliEventType::TabFocused,
            CliEvent::ModeChanged { .. } => CliEventType::ModeChanged,
            CliEvent::ClientAttached { .. } => CliEventType::ClientAttached,
            CliEvent::ClientDetached { .. } => CliEventType::ClientDetached,
            CliEvent::SessionRenamed { .. } => CliEventType::SessionRenamed,
            CliEvent::SessionEnding => CliEventType::SessionEnding,
        }
    }

    pub fn pane_id(&self) -> Option<PaneId> {
        match self {
            CliEvent::PaneOpened { pane } => Some(pane.pane_id),
            CliEvent::PaneClosed { pane_id, .. }
            | CliEvent::PaneFocused { pane_id, .. }
            | CliEvent::PaneRenamed { pane_id, .. }
            | CliEvent::PaneMoved { pane_id, .. }
            | CliEvent::PaneFloatingChanged { pane_id, .. }
            | CliEvent::PaneFullscreenChanged { pane_id, .. }
            | CliEvent::PaneExited { pane_id, .. }
            | CliEvent::PaneCwdChanged { pane_id, .. }
            | CliEvent::PaneCommandChanged { pane_id, .. } => Some(*pane_id),
            _ => None,
        }
    }

    pub fn tab_ids(&self) -> Vec<usize> {
        match self {
            CliEvent::PaneOpened { pane } => vec![pane.tab_id],
            CliEvent::PaneClosed { tab_id, .. }
            | CliEvent::PaneCwdChanged { tab_id, .. }
            | CliEvent::PaneCommandChanged { tab_id, .. } => tab_id.iter().copied().collect(),
            CliEvent::PaneFocused { tab_id, .. }
            | CliEvent::PaneRenamed { tab_id, .. }
            | CliEvent::PaneFloatingChanged { tab_id, .. }
            | CliEvent::PaneFullscreenChanged { tab_id, .. }
            | CliEvent::PaneExited { tab_id, .. }
            | CliEvent::TabClosed { tab_id, .. }
            | CliEvent::TabRenamed { tab_id, .. }
            | CliEvent::TabMoved { tab_id, .. }
            | CliEvent::TabFocused { tab_id, .. } => vec![*tab_id],
            CliEvent::PaneMoved {
                tab_id,
                previous_tab_id,
                ..
            } => vec![*tab_id, *previous_tab_id],
            CliEvent::TabCreated { tab } => vec![tab.tab_id],
            _ => vec![],
        }
    }

    pub fn is_scoped(&self) -> bool {
        self.pane_id().is_some() || !self.tab_ids().is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CliEventFilter {
    pub event_types: Vec<CliEventType>,
    pub pane_ids: Vec<PaneId>,
    pub tab_ids: Vec<usize>,
    pub initial_state: bool,
}

impl CliEventFilter {
    pub fn has_scope(&self) -> bool {
        !self.pane_ids.is_empty() || !self.tab_ids.is_empty()
    }

    pub fn accepts_type(&self, event_type: CliEventType) -> bool {
        self.event_types.is_empty() || self.event_types.contains(&event_type)
    }

    pub fn accepts_pane(&self, pane_id: PaneId, tab_id: Option<usize>) -> bool {
        !self.has_scope()
            || self.pane_ids.contains(&pane_id)
            || tab_id.map(|t| self.tab_ids.contains(&t)).unwrap_or(false)
    }

    pub fn accepts_tab(&self, tab_id: usize) -> bool {
        !self.has_scope() || self.tab_ids.contains(&tab_id)
    }

    pub fn accepts(&self, event: &CliEvent) -> bool {
        if matches!(event, CliEvent::InitialState { .. }) {
            return true;
        }
        if !self.accepts_type(event.event_type()) {
            return false;
        }
        if !self.has_scope() || !event.is_scoped() {
            return true;
        }
        if let Some(pane_id) = event.pane_id() {
            if self.pane_ids.contains(&pane_id) {
                return true;
            }
        }
        event
            .tab_ids()
            .iter()
            .any(|tab_id| self.tab_ids.contains(tab_id))
    }

    pub fn scope_initial_state(&self, event: CliEvent) -> CliEvent {
        match event {
            CliEvent::InitialState {
                session_name,
                tabs,
                panes,
                clients,
            } if self.has_scope() => {
                let panes: Vec<CliPaneState> = panes
                    .into_iter()
                    .filter(|p| self.accepts_pane(p.pane_id, Some(p.tab_id)))
                    .collect();
                let tabs = tabs
                    .into_iter()
                    .filter(|t| {
                        self.tab_ids.contains(&t.tab_id)
                            || panes.iter().any(|p| p.tab_id == t.tab_id)
                    })
                    .collect();
                CliEvent::InitialState {
                    session_name,
                    tabs,
                    panes,
                    clients,
                }
            },
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(pane_id: PaneId, tab_id: usize) -> CliPaneState {
        CliPaneState {
            pane_id,
            tab_id,
            title: "title".to_owned(),
            is_plugin: matches!(pane_id, PaneId::Plugin(_)),
            is_floating: false,
            is_fullscreen: false,
            is_suppressed: false,
            is_selectable: true,
            exited: false,
            exit_status: None,
            terminal_command: None,
            plugin_url: None,
        }
    }

    #[test]
    fn event_serializes_with_event_tag_and_string_pane_ids() {
        let event = CliEvent::PaneFocused {
            client_id: 1,
            pane_id: PaneId::Terminal(2),
            tab_id: 0,
            previous_pane_id: Some(PaneId::Plugin(3)),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["event"], "pane_focused");
        assert_eq!(json["pane_id"], "terminal_2");
        assert_eq!(json["previous_pane_id"], "plugin_3");
        let back: CliEvent = serde_json::from_value(json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn unit_event_serializes_with_event_tag() {
        let json = serde_json::to_string(&CliEvent::SessionEnding).unwrap();
        assert_eq!(json, r#"{"event":"session_ending"}"#);
    }

    #[test]
    fn event_type_names_round_trip() {
        for event_type in CliEventType::value_variants() {
            assert_eq!(
                event_type.as_str().parse::<CliEventType>().unwrap(),
                *event_type
            );
            let serialized = serde_json::to_string(event_type).unwrap();
            assert_eq!(serialized, format!("\"{}\"", event_type.as_str()));
        }
        assert!("nope".parse::<CliEventType>().is_err());
    }

    #[test]
    fn filter_by_type() {
        let filter = CliEventFilter {
            event_types: vec![CliEventType::PaneOpened],
            ..Default::default()
        };
        assert!(filter.accepts(&CliEvent::PaneOpened {
            pane: pane(PaneId::Terminal(1), 0)
        }));
        assert!(!filter.accepts(&CliEvent::SessionEnding));
        assert!(filter.accepts(&CliEvent::InitialState {
            session_name: "s".to_owned(),
            tabs: vec![],
            panes: vec![],
            clients: vec![],
        }));
    }

    #[test]
    fn initial_state_is_not_a_selectable_type() {
        assert!(!CliEventType::value_variants().contains(&CliEventType::InitialState));
        assert_eq!(
            "initial_state".parse::<CliEventType>().unwrap(),
            CliEventType::InitialState
        );
    }

    #[test]
    fn filter_by_pane_and_tab() {
        let filter = CliEventFilter {
            pane_ids: vec![PaneId::Terminal(1)],
            tab_ids: vec![5],
            ..Default::default()
        };
        assert!(filter.accepts(&CliEvent::PaneOpened {
            pane: pane(PaneId::Terminal(1), 0)
        }));
        assert!(filter.accepts(&CliEvent::PaneOpened {
            pane: pane(PaneId::Terminal(2), 5)
        }));
        assert!(!filter.accepts(&CliEvent::PaneOpened {
            pane: pane(PaneId::Terminal(2), 0)
        }));
        assert!(filter.accepts(&CliEvent::TabRenamed {
            tab_id: 5,
            name: "a".to_owned(),
            previous_name: "b".to_owned(),
        }));
        assert!(!filter.accepts(&CliEvent::TabRenamed {
            tab_id: 0,
            name: "a".to_owned(),
            previous_name: "b".to_owned(),
        }));
        assert!(filter.accepts(&CliEvent::ClientDetached { client_id: 1 }));
        assert!(filter.accepts(&CliEvent::SessionEnding));
    }

    #[test]
    fn initial_state_is_scoped_to_filter() {
        let filter = CliEventFilter {
            pane_ids: vec![PaneId::Terminal(1)],
            ..Default::default()
        };
        let tab = |tab_id: usize| CliTabState {
            tab_id,
            position: tab_id,
            name: format!("Tab #{}", tab_id),
            is_fullscreen_active: false,
            is_sync_panes_active: false,
            are_floating_panes_visible: false,
        };
        let scoped = filter.scope_initial_state(CliEvent::InitialState {
            session_name: "s".to_owned(),
            tabs: vec![tab(0), tab(1)],
            panes: vec![pane(PaneId::Terminal(1), 1), pane(PaneId::Terminal(2), 0)],
            clients: vec![],
        });
        match scoped {
            CliEvent::InitialState { tabs, panes, .. } => {
                assert_eq!(panes.len(), 1);
                assert_eq!(panes[0].pane_id, PaneId::Terminal(1));
                assert_eq!(tabs.len(), 1);
                assert_eq!(tabs[0].tab_id, 1);
            },
            other => panic!("unexpected event {:?}", other),
        }
    }
}

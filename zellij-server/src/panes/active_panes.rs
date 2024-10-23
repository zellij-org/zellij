use crate::tab::Pane;

use crate::{os_input_output::ServerOsApi, panes::PaneId, ClientId};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone)]
pub struct ActivePanes {
    active_panes: HashMap<ClientId, PaneId>,
    os_api: Box<dyn ServerOsApi>,
}

impl std::fmt::Debug for ActivePanes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.active_panes)
    }
}

impl ActivePanes {
    pub fn new(os_api: &Box<dyn ServerOsApi>) -> Self {
        let os_api = os_api.clone();
        ActivePanes {
            active_panes: HashMap::new(),
            os_api,
        }
    }
    pub fn get(&self, client_id: &ClientId) -> Option<&PaneId> {
        self.active_panes.get(client_id)
    }
    pub fn insert(
        &mut self,
        client_id: ClientId,
        pane_id: PaneId,
        panes: &mut BTreeMap<PaneId, Box<dyn Pane>>,
    ) {
        self.unfocus_pane_for_client(client_id, panes);
        self.active_panes.insert(client_id, pane_id);
        self.focus_pane(pane_id, panes);
    }
    pub fn clear(&mut self, panes: &mut BTreeMap<PaneId, Box<dyn Pane>>) {
        for pane_id in self.active_panes.values() {
            self.unfocus_pane(*pane_id, panes);
        }
        self.active_panes.clear();
    }
    pub fn is_empty(&self) -> bool {
        self.active_panes.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&ClientId, &PaneId)> {
        self.active_panes.iter()
    }
    pub fn values(&self) -> impl Iterator<Item = &PaneId> {
        self.active_panes.values()
    }
    pub fn remove(
        &mut self,
        client_id: &ClientId,
        panes: &mut BTreeMap<PaneId, Box<dyn Pane>>,
    ) -> Option<PaneId> {
        if let Some(pane_id_to_unfocus) = self.active_panes.get(&client_id) {
            self.unfocus_pane(*pane_id_to_unfocus, panes);
        }
        self.active_panes.remove(client_id)
    }
    pub fn unfocus_all_panes(&self, panes: &mut BTreeMap<PaneId, Box<dyn Pane>>) {
        for (_client_id, pane_id) in &self.active_panes {
            self.unfocus_pane(*pane_id, panes);
        }
    }
    pub fn focus_all_panes(&self, panes: &mut BTreeMap<PaneId, Box<dyn Pane>>) {
        for (_client_id, pane_id) in &self.active_panes {
            self.focus_pane(*pane_id, panes);
        }
    }
    pub fn clone_active_panes(&self) -> HashMap<ClientId, PaneId> {
        self.active_panes.clone()
    }
    pub fn contains_key(&self, client_id: &ClientId) -> bool {
        self.active_panes.contains_key(client_id)
    }
    fn unfocus_pane_for_client(
        &self,
        client_id: ClientId,
        panes: &mut BTreeMap<PaneId, Box<dyn Pane>>,
    ) {
        if let Some(pane_id_to_unfocus) = self.active_panes.get(&client_id) {
            self.unfocus_pane(*pane_id_to_unfocus, panes);
        }
    }
    fn unfocus_pane(&self, pane_id: PaneId, panes: &mut BTreeMap<PaneId, Box<dyn Pane>>) {
        if let PaneId::Terminal(terminal_id) = pane_id {
            if let Some(focus_event) = panes.get(&pane_id).and_then(|p| p.unfocus_event()) {
                let _ = self
                    .os_api
                    .write_to_tty_stdin(terminal_id, focus_event.as_bytes());
            }
        }
    }
    fn focus_pane(&self, pane_id: PaneId, panes: &mut BTreeMap<PaneId, Box<dyn Pane>>) {
        if let PaneId::Terminal(terminal_id) = pane_id {
            if let Some(focus_event) = panes.get(&pane_id).and_then(|p| p.focus_event()) {
                let _ = self
                    .os_api
                    .write_to_tty_stdin(terminal_id, focus_event.as_bytes());
            }
        }
    }
    pub fn pane_id_is_focused(&self, pane_id: &PaneId) -> bool {
        self.active_panes
            .values()
            .find(|p_id| **p_id == *pane_id)
            .is_some()
    }
}

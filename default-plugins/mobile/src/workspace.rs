use std::collections::HashMap;
use zellij_tile::prelude::*;

const WELCOME_SCREEN_PLUGIN_ALIAS: &str = "welcome-screen";

#[derive(Default)]
pub struct Workspace {
    pub own_plugin_pane_id: Option<PaneId>,
    pub tabs: Vec<TabInfo>,
    pub panes_by_tab_position: HashMap<usize, Vec<PaneInfo>>,
    pub selected_tab_position: Option<usize>,
    pub selected_pane_id: Option<PaneId>,
    pub latest_pane_contents: HashMap<PaneId, PaneContents>,
    pub mode_info: Option<ModeInfo>,
    pub session_name: Option<String>,
    pub pane_last_activity: HashMap<PaneId, u64>,
    pub pending_new_tab_position: Option<usize>,
}

impl Workspace {
    pub fn tabs_in_order(&self) -> Vec<&TabInfo> {
        let own = self.own_plugin_pane_id;
        let mut tabs: Vec<&TabInfo> = self
            .tabs
            .iter()
            .filter(|t| !self.tab_is_self_only(t.position, own))
            .collect();
        tabs.sort_by_key(|t| t.position);
        tabs
    }

    fn tab_is_self_only(&self, tab_position: usize, own: Option<PaneId>) -> bool {
        let Some(panes) = self.panes_by_tab_position.get(&tab_position) else {
            return false;
        };
        let Some(own) = own else {
            return false;
        };
        let visible: Vec<&PaneInfo> = panes.iter().filter(|p| !p.is_suppressed).collect();
        if visible.is_empty() {
            return false;
        }
        visible.iter().all(|p| pane_info_matches(p, own))
    }

    pub fn current_tab(&self) -> Option<&TabInfo> {
        let visible = self.tabs_in_order();
        if let Some(pos) = self.selected_tab_position {
            if let Some(t) = visible.iter().find(|t| t.position == pos) {
                return Some(*t);
            }
        }
        visible.first().copied()
    }

    pub fn current_tab_panes(&self) -> Vec<&PaneInfo> {
        let Some(tab) = self.current_tab() else {
            return vec![];
        };
        self.panes_for_tab(tab.position)
    }

    pub fn panes_for_tab(&self, tab_position: usize) -> Vec<&PaneInfo> {
        let own = self.own_plugin_pane_id;
        let mut panes: Vec<&PaneInfo> = self
            .panes_by_tab_position
            .get(&tab_position)
            .map(|v| v.iter().collect())
            .unwrap_or_default();
        panes.retain(|p| {
            !p.is_suppressed
                && p.is_selectable
                && !own.map(|id| pane_info_matches(p, id)).unwrap_or(false)
        });
        panes.sort_by_key(|p| (p.is_floating, p.pane_y, p.pane_x, p.id));
        panes
    }

    pub fn current_pane_viewport_len(&self) -> usize {
        self.current_pane()
            .as_ref()
            .map(pane_id_of)
            .and_then(|id| self.latest_pane_contents.get(&id))
            .map(|c| c.viewport.len())
            .unwrap_or(0)
    }

    pub fn current_pane_is_welcome(&self) -> bool {
        self.current_pane()
            .map(|p| p.is_plugin && p.plugin_url.as_deref() == Some(WELCOME_SCREEN_PLUGIN_ALIAS))
            .unwrap_or(false)
    }

    // `is_focused` is server-global (true if any client focuses the pane), so
    // selecting by it would make this client's viewport follow another
    // connected client's focus; fall back to the first pane instead.
    pub fn current_pane(&self) -> Option<PaneInfo> {
        if let Some(selected) = self.selected_pane_id {
            for pane in self.current_tab_panes() {
                if pane_info_matches(pane, selected) {
                    return Some(pane.clone());
                }
            }
        }
        self.current_tab_panes().into_iter().next().cloned()
    }
}

pub fn pane_info_matches(info: &PaneInfo, id: PaneId) -> bool {
    match id {
        PaneId::Terminal(tid) => !info.is_plugin && info.id == tid,
        PaneId::Plugin(pid) => info.is_plugin && info.id == pid,
    }
}

pub fn pane_id_of(info: &PaneInfo) -> PaneId {
    if info.is_plugin {
        PaneId::Plugin(info.id)
    } else {
        PaneId::Terminal(info.id)
    }
}

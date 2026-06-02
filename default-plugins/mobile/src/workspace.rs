//! The mobile plugin's mirror of the live Zellij session: the tabs and
//! panes the host reports, the user's internal tab/pane selection, the
//! cached ANSI viewports, and per-pane activity. Every screen reads this
//! shared model; only one screen is "active" at a time, but all of them
//! render against the same workspace.
//!
//! Kept free of host shims — these are pure accessors over the manifest
//! data. Shim-driven reconciliation lives in `pane_sync.rs`.

use std::collections::HashMap;
use zellij_tile::prelude::*;

/// Shared, screen-independent model of the session the plugin mirrors.
#[derive(Default)]
pub struct Workspace {
    /// The plugin's own pane id, fetched once on load. Used to filter
    /// the plugin out of its own tab/pane lists so we never embed our
    /// own viewport (which would feed back the previous frame's
    /// chrome into the next frame's viewport area).
    pub own_plugin_pane_id: Option<PaneId>,
    /// All tabs the plugin is aware of, in display order. Refreshed on
    /// every `TabUpdate`.
    pub tabs: Vec<TabInfo>,
    /// Panes per tab, keyed by tab position. Refreshed on every
    /// `PaneUpdate`.
    pub panes_by_tab_position: HashMap<usize, Vec<PaneInfo>>,
    /// The tab the user currently has selected in the breadcrumb. May
    /// differ from the actual focused tab. None until first selection.
    pub selected_tab_position: Option<usize>,
    /// The pane within the selected tab the user has selected. None
    /// until first selection.
    pub selected_pane_id: Option<PaneId>,
    /// Latest ANSI-formatted viewport for every pane the server is
    /// reporting on. Populated by `Event::PaneRenderReportWithAnsi`.
    pub latest_pane_contents: HashMap<PaneId, PaneContents>,
    /// Most recent mode info. Used for action labelling.
    pub mode_info: Option<ModeInfo>,
    /// Name of the session this client is attached to. Captured from
    /// `SessionUpdate` (the entry whose `is_current_session` is true).
    /// Rendered in the top bar across every screen.
    pub session_name: Option<String>,
    /// Wall-clock timestamp (unix seconds) of the most recent
    /// `PaneRenderReportWithAnsi` that included each pane. Rendered in
    /// the Panes selector's second column as a `<time> ago` relative
    /// string. Pruned alongside `latest_pane_contents` when a pane
    /// disappears from the authoritative `PaneUpdate` manifest.
    pub pane_last_activity: HashMap<PaneId, u64>,
    /// Pending tab-position auto-select after a "+ New Tab" command.
    /// `new_tab_unfocused` returns the new tab's id synchronously
    /// before the matching `TabUpdate` and `PaneUpdate` events have
    /// propagated to this plugin. We stash the new tab's position
    /// here, then on the next `PaneUpdate` we resolve it to a concrete
    /// `(tab_position, pane_id)` pair (the new tab's first pane) and
    /// clear the field.
    pub pending_new_tab_position: Option<usize>,
}

impl Workspace {
    /// Tab structs in display-order, filtered to those actually
    /// visible to this client AND not the mobile plugin's own tab.
    /// The plugin's own tab is identified by containing only the
    /// plugin's pane.
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

    /// Returns true when `tab_position`'s pane list contains only the
    /// plugin's own pane (or is empty). Used to hide the mobile
    /// plugin's own tab from selectors so we never embed ourselves.
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

    /// The currently-selected tab, falling back to the first
    /// non-mobile tab the client can see. Never returns the plugin's
    /// own tab.
    pub fn current_tab(&self) -> Option<&TabInfo> {
        let visible = self.tabs_in_order();
        if let Some(pos) = self.selected_tab_position {
            if let Some(t) = visible.iter().find(|t| t.position == pos) {
                return Some(*t);
            }
        }
        visible.first().copied()
    }

    /// Panes in the currently-selected tab, in a deterministic order.
    /// Filters out suppressed panes, unselectable panes (UI chrome like
    /// status-bar / tab-bar), and the plugin's own pane (which would
    /// never appear here in practice — `current_tab` already excludes
    /// the plugin's own tab — but the guard is cheap).
    pub fn current_tab_panes(&self) -> Vec<&PaneInfo> {
        let Some(tab) = self.current_tab() else {
            return vec![];
        };
        self.panes_for_tab(tab.position)
    }

    /// Panes for an arbitrary tab position, with the same filtering
    /// and ordering as `current_tab_panes`. Used by the overview to
    /// list panes per column without first switching the selection.
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

    /// Number of viewport lines currently cached for the selected
    /// pane. Used by the renderer to compute the bottom-anchored
    /// `skip` offset and to map the underlying pane's cursor row into
    /// plugin-render coordinates.
    pub fn current_pane_viewport_len(&self) -> usize {
        self.current_pane()
            .as_ref()
            .map(pane_id_of)
            .and_then(|id| self.latest_pane_contents.get(&id))
            .map(|c| c.viewport.len())
            .unwrap_or(0)
    }

    /// True when the currently-selected pane is the session-manager
    /// plugin launched via the `welcome-screen` alias (i.e. the
    /// welcome layout's only pane). Identified by
    /// `PaneInfo.plugin_url`, which carries the alias name
    /// `"welcome-screen"` for panes spawned from the alias — see
    /// `RunPluginOrAlias::location_string` in
    /// `zellij-utils/src/input/layout.rs`. Plain session-manager
    /// invocations (e.g. opened mid-session via keybinding) report a
    /// different `plugin_url`, so this check does not catch them.
    pub fn current_pane_is_welcome(&self) -> bool {
        self.current_pane()
            .map(|p| {
                p.is_plugin
                    && p.plugin_url.as_deref() == Some("welcome-screen")
            })
            .unwrap_or(false)
    }

    /// Currently-selected pane info, falling back to the first pane in
    /// the selected tab. We deliberately do NOT fall back to
    /// `is_focused` — that flag is global on the server (true if any
    /// client focuses the pane, see `ActivePanes::pane_id_is_focused`),
    /// so using it here would make the embedded viewport track another
    /// connected client's focus changes.
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

/// Match a `PaneInfo` against a `PaneId`. The plugin event surface
/// reports id + is_plugin separately; the server-side enum carries the
/// same distinction.
pub fn pane_info_matches(info: &PaneInfo, id: PaneId) -> bool {
    match id {
        PaneId::Terminal(tid) => !info.is_plugin && info.id == tid,
        PaneId::Plugin(pid) => info.is_plugin && info.id == pid,
    }
}

/// Build a `PaneId` from a `PaneInfo`.
pub fn pane_id_of(info: &PaneInfo) -> PaneId {
    if info.is_plugin {
        PaneId::Plugin(info.id)
    } else {
        PaneId::Terminal(info.id)
    }
}

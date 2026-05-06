//! State for the mobile UI plugin. Tracks tabs, panes, the user's
//! current selection, the cached ANSI viewports, and the click-region
//! map produced by the renderer for mouse-event dispatch.

use std::collections::HashMap;
use zellij_tile::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selector {
    Tabs,
    Panes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickAction {
    ExpandTabs,
    ExpandPanes,
    Collapse,
    SelectTab(usize),         // tab position (0-based)
    SelectPane(PaneId),
    ToggleType,
    NewPane,
    NewTab,
    SplitRight,
    SplitDown,
    ToggleFloating,
    CloseFocus,
    Detach,
    ExitMobile,
}

#[derive(Debug, Clone)]
pub struct ClickRegion {
    pub row: usize,
    pub col_start: usize,
    pub col_end: usize, // exclusive
    pub action: ClickAction,
}

/// Where the embedded pane viewport sits within the plugin render area
/// on the most recent frame. The mouse handler uses this to translate
/// a click in plugin coordinates back into the underlying pane's
/// viewport-row / viewport-col so it can synthesize an SGR mouse press
/// that lands at the equivalent cell in the pane's pty.
#[derive(Debug, Clone, Copy)]
pub struct ViewportRegion {
    /// Top row of the embedded viewport in plugin coordinates (0-based).
    pub row_start: usize,
    /// Bottom row (exclusive) of the embedded viewport in plugin coords.
    pub row_end: usize,
    /// Width in cells of the embedded viewport.
    pub cols: usize,
    /// How many leading lines were sliced off the cached viewport when
    /// rendering — i.e. the offset into `PaneContents.viewport` that
    /// corresponds to `row_start`. Anchored at the bottom: when the
    /// pane is taller than `row_end - row_start` we show the last
    /// `row_end - row_start` lines.
    pub skip: usize,
}

#[derive(Default)]
pub struct State {
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
    /// Which selector (if any) is currently expanded. None = collapsed
    /// view (the embedded viewport dominates the screen).
    pub expanded: Option<Selector>,
    /// Latest ANSI-formatted viewport for every pane the server is
    /// reporting on. Populated by `Event::PaneRenderReportWithAnsi`.
    pub latest_pane_contents: HashMap<PaneId, PaneContents>,
    /// Most recent mode info. Used for action labelling.
    pub mode_info: Option<ModeInfo>,
    /// Whether typing-mode is armed. Wired in Stage 7; for now the
    /// action bar surfaces the toggle but key events stay with the
    /// plugin.
    pub typing_mode: bool,
    /// Click regions produced by the most recent render. The renderer
    /// rebuilds this on every `render` call; mouse events look up the
    /// hit region by (row, col).
    pub click_regions: Vec<ClickRegion>,
    /// Where the embedded viewport ended up on the most recent render.
    /// Set by the renderer; consumed by the mouse handler to dispatch
    /// viewport-passthrough clicks to the underlying pane.
    pub viewport_region: Option<ViewportRegion>,
    /// Last `show_cursor` payload the plugin emitted to the host.
    /// Calling `show_cursor` is *not* idempotent on the server side:
    /// `ScreenInstruction::ShowPluginCursor` triggers a full
    /// `screen.render` and a `log_and_report_session_state`, which
    /// then produces a fresh `PaneRenderReportWithAnsi` for the
    /// plugin — feeding back into the plugin's render loop. We
    /// therefore cache the last value sent and only re-emit when the
    /// target position would actually change.
    pub last_emitted_cursor: LastEmittedCursor,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LastEmittedCursor {
    /// No `show_cursor` call has been made yet — the next render must
    /// emit unconditionally so the host's initial cursor state matches
    /// what the plugin has computed.
    #[default]
    Unknown,
    /// The most recent `show_cursor` payload — `None` for "hidden",
    /// `Some((x, y))` for "shown at these plugin-coords".
    Sent(Option<(usize, usize)>),
}

impl State {
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
        let own = self.own_plugin_pane_id;
        let mut panes: Vec<&PaneInfo> = self
            .panes_by_tab_position
            .get(&tab.position)
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

    /// If a click at (row, col) lands inside the most recently rendered
    /// embedded-viewport region, return the equivalent (row, col) in
    /// the underlying pane's viewport coordinates (0-based, relative to
    /// the top-left of the cached pane viewport). Returns `None` if the
    /// click is outside the viewport area or no viewport has been
    /// rendered yet.
    pub fn click_in_viewport(&self, row: usize, col: usize) -> Option<(usize, usize)> {
        let region = self.viewport_region?;
        if row < region.row_start || row >= region.row_end {
            return None;
        }
        if col >= region.cols {
            return None;
        }
        let pane_row = region.skip + (row - region.row_start);
        Some((pane_row, col))
    }

    /// Resolve a click at (row, col) to the action it should fire, if
    /// any. Returns the first hit; click regions are inserted in
    /// front-to-back order so the renderer should not place
    /// overlapping regions.
    pub fn click_to_action(&self, row: usize, col: usize) -> Option<ClickAction> {
        for region in &self.click_regions {
            if region.row == row && col >= region.col_start && col < region.col_end {
                return Some(region.action);
            }
        }
        None
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

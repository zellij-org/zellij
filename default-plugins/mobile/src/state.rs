//! State for the mobile UI plugin. Tracks tabs, panes, the user's
//! current selection, the cached ANSI viewports, and the click-region
//! map produced by the renderer for mouse-event dispatch.

use std::collections::HashMap;
use std::time::Instant;
use zellij_tile::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selector {
    /// Sessions list — col 1 is the session name, col 2 is the
    /// session's tab + pane counts. Selecting a row calls
    /// `switch_session`.
    Sessions,
    /// Tabs list — col 1 is the tab name, col 2 is the tab's pane
    /// count. Selecting a row updates `selected_tab_position`.
    Tabs,
    /// Panes list (panes of the currently-selected tab). Col 1 is the
    /// pane title, col 2 is the pane's last-activity timestamp
    /// rendered as a relative `<time> ago` string.
    Panes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickAction {
    /// Open the sessions selector.
    ExpandSessions,
    /// Open the tabs selector.
    ExpandTabs,
    /// Open the panes selector (panes of the currently-selected tab).
    ExpandPanes,
    /// Close any open selector without changing the current selection.
    /// Used by the in-selector top-bar tap (escape hatch back to the
    /// embedded viewport).
    CollapseSelector,
    /// Selecting a session calls `switch_session(name)` on the host —
    /// the client genuinely changes session, leaving this one.
    SelectSession(String),
    /// Tap a tab row in the Tabs selector. Updates the plugin's
    /// internal `selected_tab_position` (does NOT change the client's
    /// actual focused tab — that would dismount the mobile plugin
    /// itself).
    SelectTab(usize),
    /// Tap a pane row in the Panes selector. Updates the plugin's
    /// internal `selected_tab_position` + `selected_pane_id` so the
    /// embedded viewport shows that pane (the panes selector lists
    /// panes from every tab, so the click must restate which tab the
    /// chosen pane lives on).
    SelectPane {
        tab_position: usize,
        pane_id: PaneId,
    },
    ToggleType,
    /// Tap on a shortcut in the bottom bar. The usize indexes into
    /// `state.bottom_bar_shortcuts`. The handler reads the shortcut's
    /// `action` field, dispatches it, and stamps `pressed_at` so the
    /// renderer can paint the brief 400 ms visual feedback.
    BottomBarShortcut(usize),
}

/// What a bottom-bar shortcut does when tapped. Each variant maps to
/// a concrete write/dispatch in `dispatch_click`. Adding a new
/// shortcut: extend this enum and the dispatch arm; the rendering and
/// click-region plumbing is data-driven and needs no other change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BottomBarAction {
    /// Send a bare key (Esc, Tab, arrows, plain `-`, etc.) to the
    /// pane currently shown in the embedded viewport. Any sticky
    /// modifiers that are held (`ctrl_held` / `alt_held`) are folded
    /// in before serialization and then cleared, mimicking the
    /// "press-and-release" behaviour of a hardware keyboard.
    SendKey(BareKey),
    /// Sticky-modifier toggle for Ctrl. Tapping this once arms the
    /// modifier; the next `SendKey` (from this bar or from a typed
    /// soft-keyboard event in typing mode) consumes and clears it.
    /// Tapping again disarms without sending anything.
    ToggleCtrl,
    /// Sticky-modifier toggle for Alt. Same semantics as `ToggleCtrl`.
    ToggleAlt,
}

/// One entry in the bottom bar. The renderer paints the label colour
/// at index 3 normally, switching to index 2 for the brief window
/// after the user taps it (`pressed_at = Some(_)` and elapsed < 400 ms).
#[derive(Debug, Clone)]
pub struct BottomBarShortcut {
    pub label: String,
    pub action: BottomBarAction,
    /// When the shortcut was most recently tapped. Cleared by the
    /// `Event::Timer` sweep in `update` after the feedback window has
    /// elapsed. `None` outside the feedback window.
    pub pressed_at: Option<Instant>,
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
    /// Name of the session this client is attached to. Captured from
    /// `SessionUpdate` (the entry whose `is_current_session` is true).
    /// Rendered in the top bar.
    pub session_name: Option<String>,
    /// All sessions the host knows about. Updated on every
    /// `SessionUpdate`. Rendered when the session selector is open.
    pub sessions: Vec<SessionInfo>,
    /// Whether the plugin forwards received keystrokes to the
    /// selected pane's pty. Now permanently set to `true` in `load()`
    /// — soft keyboard input always reaches the embedded program by
    /// default. The field is kept (rather than removed) so future
    /// affordances can re-introduce a swallow-keys mode if needed.
    pub typing_mode: bool,
    /// Whether the soft keyboard is currently up on the calling web
    /// client's browser. Driven from the plugin side: tapping the ⌨
    /// glyph in the top bar flips this and emits a `set_soft_keyboard`
    /// shim call so the browser shows or hides its on-screen keyboard
    /// to match. Always `false` on terminal clients (the IPC message
    /// is swallowed there) but the field is still tracked so the
    /// top-bar indicator can render consistently across clients.
    pub soft_keyboard_visible: bool,
    /// Click regions produced by the most recent render. The renderer
    /// rebuilds this on every `render` call; mouse events look up the
    /// hit region by (row, col).
    pub click_regions: Vec<ClickRegion>,
    /// Where the embedded viewport ended up on the most recent render.
    /// Set by the renderer; consumed by the mouse handler to dispatch
    /// viewport-passthrough clicks to the underlying pane.
    pub viewport_region: Option<ViewportRegion>,
    /// Wall-clock timestamp (unix seconds) of the most recent
    /// `PaneRenderReportWithAnsi` that included each pane. Updated
    /// every time the host reports a content change for that pane;
    /// rendered in the Panes selector's second column as a
    /// `<time> ago` relative string. Pruned alongside
    /// `latest_pane_contents` when a pane disappears from the
    /// authoritative `PaneUpdate` manifest.
    pub pane_last_activity: HashMap<PaneId, u64>,
    /// Bottom-bar shortcuts rendered as a pipe-separated list in the
    /// chrome row at the bottom of the plugin's render area. Populated
    /// once on `load`; the order in this `Vec` is the visual order on
    /// screen, and the index is the click-region's identifier.
    pub bottom_bar_shortcuts: Vec<BottomBarShortcut>,
    /// Sticky-Ctrl flag. Set by tapping the `CTRL` shortcut; cleared
    /// when the next non-modifier key (from the bar or from a
    /// typing-mode `Event::Key`) consumes it. Rendered as the `CTRL`
    /// label's "active" colour while set.
    pub ctrl_held: bool,
    /// Sticky-Alt flag. Same semantics as `ctrl_held`.
    pub alt_held: bool,
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
                return Some(region.action.clone());
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

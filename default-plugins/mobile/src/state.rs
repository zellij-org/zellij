//! State for the mobile UI plugin. Tracks tabs, panes, the user's
//! current selection, the cached ANSI viewports, and the click-region
//! map produced by the renderer for mouse-event dispatch.

use std::collections::HashMap;
use zellij_tile::prelude::*;

use crate::keyboard::{CellId, KeyboardController};

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
    /// Tap on the keyboard glyph in the top bar. Toggles the plugin
    /// keyboard's visibility (`KeyboardController::visible`) and emits
    /// `set_soft_keyboard(true|false)` to suppress the OS soft
    /// keyboard whenever the plugin keyboard is showing.
    ToggleKeyboard,
    /// Tap on the fit (⛶) glyph in the top bar. Toggles a server-
    /// side per-tab override that resizes the focused pane's tab to
    /// match this plugin's embedded viewport area, fullscreening
    /// the pane in the process. The plugin keeps a mirror
    /// (`State::fit_active`) so the next tap takes the off path.
    /// See `enter_fit_mode` / `exit_fit_mode` in the shim.
    ToggleFit,
    /// Tap on a cell of the in-plugin keyboard. Routed through
    /// `KeyboardController::handle_tap` which resolves to bytes,
    /// modifier toggles, or a visibility flip.
    Keyboard(CellId),
}

/// A rectangular click target with a priority for layered scanning.
///
/// **Tight** regions (`priority == 0`) cover the visible interior of
/// a cell. They never overlap in a well-formed render and are
/// scanned first — a hit returns immediately. Used for chrome and
/// for the keyboard's visible cells.
///
/// **Slop** regions (`priority > 0`) cover the hit-slop halo around
/// small targets like keyboard cells. They overlap with sibling
/// slop regions on shared dividers / walls; the dispatcher resolves
/// the ambiguity by nearest-center (Euclidean squared distance from
/// the click to `center`). Ties break lex-first by `(center_y,
/// center_x)`, which on vertical ties prefers the cell whose content
/// row is higher up — matches the "users overshoot down on the
/// keyboard" intuition.
#[derive(Debug, Clone)]
pub struct ClickRegion {
    /// Inclusive top row of the region in plugin coordinates.
    pub row_start: usize,
    /// Exclusive bottom row of the region in plugin coordinates. For
    /// single-row click targets (chrome, selectors, single-row
    /// keyboard rows) `row_end == row_start + 1`. For the option-2b
    /// tall keyboard cells `row_end == row_start + 2` so the entire
    /// padding-plus-label rectangle counts as one tap target.
    pub row_end: usize,
    pub col_start: usize,
    pub col_end: usize, // exclusive
    pub action: ClickAction,
    /// 0 = tight (exact / no overlap); higher = slop (overlaps OK).
    pub priority: u8,
    /// Geometric center of the *visible* cell this region belongs
    /// to. Required for `priority > 0` so nearest-center can break
    /// overlap ties; ignored otherwise.
    pub center: Option<(usize, usize)>,
}

impl ClickRegion {
    /// Construct a single-row tight region — scanned first; first hit
    /// wins. Used by the chrome (top bar, selectors) where every
    /// click target occupies a single terminal row.
    pub fn tight(
        row: usize,
        col_start: usize,
        col_end: usize,
        action: ClickAction,
    ) -> Self {
        Self::tight_range(row, row + 1, col_start, col_end, action)
    }

    /// Construct a multi-row tight region — same semantics as
    /// `tight`, but the vertical extent covers `[row_start, row_end)`.
    /// Used by tall keyboard cells so a tap anywhere inside the cell's
    /// padding-plus-label rectangle hits the cell directly without
    /// falling back to slop dispatch.
    pub fn tight_range(
        row_start: usize,
        row_end: usize,
        col_start: usize,
        col_end: usize,
        action: ClickAction,
    ) -> Self {
        Self {
            row_start,
            row_end,
            col_start,
            col_end,
            action,
            priority: 0,
            center: None,
        }
    }

    /// Construct a single-row slop region — scanned only if no tight
    /// region matched; overlapping siblings resolved by nearest-center.
    pub fn slop(
        row: usize,
        col_start: usize,
        col_end: usize,
        action: ClickAction,
        center: (usize, usize),
    ) -> Self {
        Self::slop_range(row, row + 1, col_start, col_end, action, center)
    }

    /// Construct a multi-row slop region. Used by tall keyboard cells
    /// so the slop halo extends ±`SLOP_V` rows around the cell's
    /// outer rectangle, not just around the label row.
    pub fn slop_range(
        row_start: usize,
        row_end: usize,
        col_start: usize,
        col_end: usize,
        action: ClickAction,
        center: (usize, usize),
    ) -> Self {
        Self {
            row_start,
            row_end,
            col_start,
            col_end,
            action,
            priority: 1,
            center: Some(center),
        }
    }
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
    /// `row_end - row_start` lines, minus any active vertical pan.
    pub skip: usize,
    /// How many leading cells were sliced off each cached viewport
    /// line when rendering — i.e. the offset into each line that
    /// corresponds to col 0 of the embedded viewport. 0 when the pane
    /// fits horizontally or the user has not panned right yet.
    pub h_offset: usize,
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
    /// In-plugin on-screen keyboard. Owns the active layout, modifier
    /// flags (Shift/Ctrl/Alt — one-shot — plus Fn and Layer123 as
    /// sticky toggles), press-flash timestamps and visibility.
    /// Visible by default — the corresponding OS soft-keyboard
    /// suppression is emitted from `load()`.
    pub keyboard: KeyboardController,
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
    /// Sticky-Ctrl flag. Aliased to `KeyboardController::modifiers
    /// .ctrl_armed` — the keyboard's tap handler reads/writes it
    /// through a `&mut` reference so the hardware-key passthrough
    /// path and the plugin keyboard share the same one-shot state.
    /// Consumed by the next non-modifier key from either path.
    pub ctrl_held: bool,
    /// Sticky-Alt flag. Same semantics as `ctrl_held`.
    pub alt_held: bool,
    /// Rows panned UP from the bottom-anchored default — 0 means
    /// "follow latest" (current behaviour). When > 0 the rendered
    /// slice's bottom edge sits this many rows above the viewport's
    /// last line. Capped at `viewport.len().saturating_sub(height)`
    /// inside the renderer so transient values that exceed the new
    /// maximum (after a resize, or after lines fall off the top of
    /// the viewport) clamp gracefully without forcing follow-mode.
    pub viewport_v_pan: usize,
    /// Cols panned RIGHT from the left edge — 0 = leftmost (current
    /// behaviour). When > 0 the rendered slice's left edge sits this
    /// many cells into each cached viewport line. Capped at
    /// `pane_content_columns.saturating_sub(cols)` inside the
    /// renderer.
    pub viewport_h_pan: usize,
    /// True while Fit mode is active (the ⛶ glyph is "armed"). The
    /// authoritative state lives on the server (`Screen::fit_states`)
    /// — this is just the plugin's mirror so the next tap on the
    /// glyph takes the off path and the renderer colours the glyph.
    /// Reset to `false` when the user picks a different tab or pane,
    /// or when the previously-selected pane disappears.
    pub fit_active: bool,
    /// The tab the local fit is bound to. Set when `EnterFitMode` is
    /// sent; cleared by every path that clears `fit_active`. Threaded
    /// through `UpdateFitSize` so the server can look up the override
    /// entry by tab_id (rather than by owning_client), which is what
    /// lets a displaced client (whose entry was overwritten by a
    /// colliding fit on the same tab) reclaim ownership on its next
    /// push.
    pub fit_tab_id: Option<usize>,
    /// The (rows, cols) most recently sent to the server via
    /// `enter_fit_mode` or `update_fit_size`. Diffed against
    /// `fit_pending_target` in `update()` so we only re-send when
    /// the embedded viewport has actually moved.
    pub fit_last_sent_size: Option<(usize, usize)>,
    /// The target tab size last computed by `render_embedded_viewport`
    /// based on the most recent embedded-viewport area + cached pane
    /// /tab geometry. Set during render but the matching shim call
    /// is deferred to `update()` — calling `update_fit_size` from
    /// inside render corrupts `print!` output because
    /// `host_run_plugin_command` drains the entire plugin stdout
    /// pipe via `read_to_end` (see `wasi_read_bytes` /
    /// `host_run_plugin_command` in `zellij_exports.rs`), so any
    /// rendered bytes already written would be consumed and the
    /// JSON-decode of the protobuf would fail on the mixed payload.
    /// `update()` runs with a clean stdout, so calling the shim
    /// from there is safe. The compare/send happens on every event
    /// — by the time the next event fires after a pinch (TabUpdate
    /// or PaneUpdate from the same RecomputeTabSize handler that
    /// emitted the resize), this field reflects the new embedded
    /// area and the shim is dispatched.
    pub fit_pending_target: Option<(usize, usize)>,
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
        let pane_col = region.h_offset + col;
        Some((pane_row, pane_col))
    }

    /// Resolve a click at (row, col) to the action it should fire,
    /// if any.
    ///
    /// Pass 1 scans **tight** regions (priority 0): first hit wins.
    /// Tight regions are guaranteed non-overlapping by the renderer,
    /// so order does not matter — a hit there resolves the click
    /// unambiguously.
    ///
    /// Pass 2 scans **slop** regions (priority > 0). Slop regions
    /// may overlap on shared boundaries (walls, dividers); the
    /// candidate whose `center` is closest to the click — by squared
    /// Euclidean distance — wins. Ties break lex-first by
    /// `(center_y, center_x)` so the result is deterministic.
    pub fn click_to_action(&self, row: usize, col: usize) -> Option<ClickAction> {
        // Pass 1: tight regions.
        for region in &self.click_regions {
            if region.priority == 0
                && region.row_start <= row
                && row < region.row_end
                && col >= region.col_start
                && col < region.col_end
            {
                return Some(region.action.clone());
            }
        }
        // Pass 2: slop regions, resolved by nearest-center.
        let mut best: Option<(&ClickRegion, u64)> = None;
        for region in &self.click_regions {
            if region.priority == 0 {
                continue;
            }
            if row < region.row_start || row >= region.row_end {
                continue;
            }
            if col < region.col_start || col >= region.col_end {
                continue;
            }
            let Some((cx, cy)) = region.center else { continue };
            let dx = (cx as i64 - col as i64).unsigned_abs();
            let dy = (cy as i64 - row as i64).unsigned_abs();
            let dist_sq = dx * dx + dy * dy;
            best = Some(match best {
                None => (region, dist_sq),
                Some((cur, cur_d)) if dist_sq < cur_d => (region, dist_sq),
                Some((cur, cur_d)) if dist_sq == cur_d => {
                    let cur_key = slop_key(cur);
                    let new_key = slop_key(region);
                    if new_key < cur_key {
                        (region, dist_sq)
                    } else {
                        (cur, cur_d)
                    }
                },
                Some(prev) => prev,
            });
        }
        best.map(|(r, _)| r.action.clone())
    }
}

/// Deterministic tiebreaker key for overlapping slop regions. Lex
/// on `(center_y, center_x)`, falling back to the region's own
/// position when no center is present (should not happen for
/// well-formed slop regions, but defensive).
fn slop_key(r: &ClickRegion) -> (usize, usize) {
    match r.center {
        Some((cx, cy)) => (cy, cx),
        None => (r.row_start, r.col_start),
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

#[cfg(test)]
mod tests {
    //! Dispatch tests for the layered tight/slop priority system.
    //!
    //! `ClickAction::Keyboard` cases use `CellId` values straight
    //! from the keyboard layout space; the dispatcher does not
    //! interpret them so any u16 works.
    use super::*;
    use crate::keyboard::CellId;

    fn kb(id: u16) -> ClickAction {
        ClickAction::Keyboard(CellId(id))
    }

    /// A tight hit on a cell resolves to that cell even if a sibling
    /// cell's slop region also covers the click coordinate.
    #[test]
    fn tight_wins_over_overlapping_slop() {
        let mut s = State::default();
        // Cell A at (row 5, cols 10..13), center (11, 5).
        s.click_regions.push(ClickRegion::tight(5, 10, 13, kb(1)));
        s.click_regions
            .push(ClickRegion::slop(5, 9, 14, kb(1), (11, 5)));
        // Cell B at (row 5, cols 13..16), center (14, 5).
        s.click_regions.push(ClickRegion::tight(5, 13, 16, kb(2)));
        s.click_regions
            .push(ClickRegion::slop(5, 12, 17, kb(2), (14, 5)));

        // Click at col 12 — inside A's tight region; B's slop also
        // matches but tight takes precedence.
        assert_eq!(s.click_to_action(5, 12), Some(kb(1)));
        // Click at col 13 — inside B's tight region.
        assert_eq!(s.click_to_action(5, 13), Some(kb(2)));
    }

    /// A click that misses every tight region falls back to slop,
    /// resolved by nearest-center.
    #[test]
    fn slop_resolves_by_nearest_center() {
        let mut s = State::default();
        // Two cells stacked vertically, content rows 5 and 7,
        // sharing the divider at row 6.
        // Cell A: tight (5, 10..13), center (11, 5).
        s.click_regions.push(ClickRegion::tight(5, 10, 13, kb(1)));
        // A's slop spans rows 4..=6, cols 9..14.
        for r in 4..=6 {
            s.click_regions
                .push(ClickRegion::slop(r, 9, 14, kb(1), (11, 5)));
        }
        // Cell B: tight (7, 10..13), center (11, 7).
        s.click_regions.push(ClickRegion::tight(7, 10, 13, kb(2)));
        for r in 6..=8 {
            s.click_regions
                .push(ClickRegion::slop(r, 9, 14, kb(2), (11, 7)));
        }

        // Click on the divider (row 6, col 11) — equidistant from
        // both centers vertically; tiebreaker prefers the upper
        // (smaller cy) cell A.
        assert_eq!(s.click_to_action(6, 11), Some(kb(1)));
        // Click clearly closer to B (row 8) — but the only region
        // matching at (8, 11) is B's slop.
        assert_eq!(s.click_to_action(8, 11), Some(kb(2)));
    }

    /// Clicks outside every region return None.
    #[test]
    fn miss_returns_none() {
        let mut s = State::default();
        s.click_regions.push(ClickRegion::tight(5, 10, 13, kb(1)));
        s.click_regions
            .push(ClickRegion::slop(5, 9, 14, kb(1), (11, 5)));
        assert!(s.click_to_action(0, 0).is_none());
        assert!(s.click_to_action(5, 20).is_none());
    }

    /// Manually arm fit state (as the `ToggleFit` ON path would) then
    /// exercise the OFF path via `clear_fit_if_active`. All four fit
    /// fields must reset to their default off-state values regardless
    /// of which subset was set on entry.
    #[test]
    fn toggle_fit_field_round_trip() {
        let mut s = State::default();
        s.fit_active = true;
        s.fit_last_sent_size = Some((10, 40));
        s.fit_pending_target = Some((10, 40));
        s.fit_tab_id = Some(7);
        crate::clear_fit_if_active(&mut s);
        assert!(!s.fit_active);
        assert_eq!(s.fit_last_sent_size, None);
        assert_eq!(s.fit_pending_target, None);
        assert_eq!(s.fit_tab_id, None);

        // Calling clear again while inactive is a no-op (no panic, no
        // mutation) — required because the dispatch paths invoke this
        // unconditionally on tab/pane switch.
        crate::clear_fit_if_active(&mut s);
        assert!(!s.fit_active);
        assert_eq!(s.fit_last_sent_size, None);
        assert_eq!(s.fit_pending_target, None);
        assert_eq!(s.fit_tab_id, None);
    }
}

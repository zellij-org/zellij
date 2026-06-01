//! State for the mobile UI plugin. Tracks tabs, panes, the user's
//! current selection, the cached ANSI viewports, and the click-region
//! map produced by the renderer for mouse-event dispatch.

use std::collections::HashMap;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use zellij_tile::prelude::*;

use crate::modifier_bar::{CellId, ModifierBarController};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selector {
    /// Sessions list — col 1 is the session name, col 2 is the
    /// session's tab + pane counts. Selecting a row calls
    /// `switch_session`.
    Sessions,
    /// Unified pane navigator. Panes are listed grouped under their
    /// tab as nested rows: a tab-name header followed by the panes
    /// belonging to that tab in display order. Only pane rows are
    /// clickable — selecting one updates the embedded viewport. The
    /// header rows are visual nesting and carry no action.
    Panes,
    /// In-plugin name-entry overlay for creating a new session.
    /// Submitting (Enter) calls `switch_session(Some(buf))` — or
    /// `switch_session(None)` when the buffer is empty, mirroring the
    /// session-manager plugin's "leave name empty for auto-name"
    /// behaviour. Esc dismisses without creating a session. While this
    /// variant is active, the `Event::Key` handler intercepts every
    /// key (character/Backspace/Enter/Esc) instead of forwarding it
    /// to the embedded pane.
    NewSessionPrompt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickAction {
    /// Open the sessions selector.
    ExpandSessions,
    /// Open the unified pane navigator (panes grouped under tabs).
    ExpandPanes,
    /// Close any open selector without changing the current selection.
    /// Used by the in-selector top-bar tap (escape hatch back to the
    /// embedded viewport).
    CollapseSelector,
    /// Selecting a session calls `switch_session(name)` on the host —
    /// the client genuinely changes session, leaving this one.
    SelectSession(String),
    /// Tap a pane row in the Panes selector. Updates the plugin's
    /// internal `selected_tab_position` + `selected_pane_id` so the
    /// embedded viewport shows that pane (the panes selector lists
    /// panes from every tab, so the click must restate which tab the
    /// chosen pane lives on).
    SelectPane {
        tab_position: usize,
        pane_id: PaneId,
    },
    /// Tap on the fit (⛶) glyph in the top bar. Toggles a server-
    /// side per-tab override that resizes the focused pane's tab to
    /// match this plugin's embedded viewport area, fullscreening
    /// the pane in the process. The plugin keeps a mirror
    /// (`State::fit_active`) so the next tap takes the off path.
    /// See `enter_fit_mode` / `exit_fit_mode` in the shim.
    ToggleFit,
    /// Tap on a cell of the modifier bar at the bottom of the plugin
    /// area. Routed through `ModifierBarController::handle_tap` which
    /// resolves to bytes (for ESC, TAB, arrows, `-`) or a modifier
    /// flip (for CTRL, ALT).
    Keyboard(CellId),
    /// Tap on the hamburger glyph in the top bar. Flips
    /// `State::menu_open`. The dropdown menu and the selector menus
    /// (`State::expanded`) are mutually exclusive — opening any
    /// selector also clears `menu_open`, and the menu render is
    /// gated on `state.expanded.is_none()`.
    ToggleMenu,
    /// Tap on a "+ New Pane" row in the Panes selector. Calls the
    /// `new_tiled_pane_in_tab` shim which dispatches
    /// `Action::NewTiledPane { tab_id: Some(tab_position), .. }` on the
    /// server and returns the new pane id synchronously. The plugin
    /// auto-selects the new pane and closes the selector.
    NewPaneInTab { tab_position: usize },
    /// Tap on the "+ New Tab" row at the bottom of the Panes selector.
    /// Calls the `new_tab_unfocused` shim which dispatches
    /// `Action::NewTab { should_change_focus_to_new_tab: false, .. }`
    /// on the server and returns the new tab id synchronously, so the
    /// client never leaves its current (mobile plugin) tab. The plugin
    /// auto-selects the new tab once its first pane shows up in the
    /// next `PaneUpdate`.
    NewTab,
    /// Tap on the "+ New Session" row at the bottom of the Sessions
    /// selector. Switches `state.expanded` to
    /// `Selector::NewSessionPrompt` and clears
    /// `state.pending_session_name` so the overlay starts with an
    /// empty buffer regardless of any previously-cancelled attempt.
    /// No host call is made here — the actual `switch_session` is
    /// dispatched from the prompt's Enter handler in `Event::Key`.
    OpenNewSessionPrompt,
    /// Tap on the "[Cancel]" button in the New Session prompt.
    /// Mirrors the prompt's Esc key handler: clears the pending
    /// buffer and closes the prompt without calling the host. Kept
    /// distinct from `CollapseSelector` because the latter does not
    /// clear the buffer (and a stale buffer leaking into a future
    /// open would surprise the user).
    CancelNewSessionPrompt,
    /// Tap on the "[Accept]" button in the New Session prompt.
    /// Mirrors the prompt's Enter key handler: hands the buffer to
    /// `switch_session` (empty buffer → host picks an auto-name) and
    /// closes the prompt. The mobile plugin then dismounts as the
    /// host swaps the client into the new session.
    AcceptNewSessionPrompt,
    /// Tap on the "Switch to Desktop" hamburger menu row. Calls the
    /// `exit_mobile_mode` shim, which tears down this client's
    /// mobile tab server-side. The mobile UI dismounts as the tab
    /// closes; re-entry is via reconnect / refresh (which
    /// re-triggers the server's auto-mobile-mode detection).
    ExitMobileMode,
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
    /// True while the hamburger dropdown menu is open. Mutually
    /// exclusive with `expanded`: opening any selector clears this,
    /// and the menu render is gated on `expanded.is_none()`. The
    /// menu overlays the upper-right corner of the embedded viewport
    /// when open.
    pub menu_open: bool,
    /// Scroll offset into the currently-open selector's row list. 0
    /// = first row visible at the top of the body region. Reset to 0
    /// every time a selector is opened so each entry into Change
    /// Pane / Change Session begins anchored at the top. The
    /// renderer clamps stale values against the row list's actual
    /// length on the next frame, so handlers can blindly increment
    /// past the maximum without producing a glitch.
    pub selector_scroll_offset: usize,
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
    /// sticky toggles) and visibility. Visible by default — the
    /// corresponding OS soft-keyboard suppression is emitted from
    /// `load()`.
    pub modifier_bar: ModifierBarController,
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
    /// Sticky-Ctrl flag. Aliased to `ModifierBarController::modifiers
    /// .ctrl_armed` — the bar's tap handler reads/writes it
    /// through a `&mut` reference so the hardware-key passthrough
    /// path and the modifier bar share the same one-shot state.
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
    /// through `UpdateFitInsets` so the server can look up the override
    /// entry by tab_id (rather than by owning_client), which is what
    /// lets a displaced client (whose entry was overwritten by a
    /// colliding fit on the same tab) reclaim ownership on its next
    /// push. The plugin no longer computes or mirrors the target size:
    /// it reports its chrome insets (top bar, soft-keyboard bar) via
    /// `enter_fit_mode` / `update_fit_insets` and the server derives the
    /// size from live geometry. Insets depend only on semantic UI state,
    /// all mutated in `update()`, so they are emitted inline from the
    /// arm that changes them — no render-stash-then-flush is needed.
    pub fit_tab_id: Option<usize>,
    /// Pending tab-position auto-select after a "+ New Tab" command.
    /// `new_tab_unfocused` returns the new tab's id synchronously
    /// before the matching `TabUpdate` and `PaneUpdate` events have
    /// propagated to this plugin. We stash the new tab's position
    /// here, then on the next `PaneUpdate` we resolve it to a concrete
    /// `(tab_position, pane_id)` pair (the new tab's first pane) and
    /// clear the field.
    pub pending_new_tab_position: Option<usize>,
    /// Last `show_cursor` payload the plugin emitted to the host.
    /// Calling `show_cursor` is *not* idempotent on the server side:
    /// `ScreenInstruction::ShowPluginCursor` triggers a full
    /// `screen.render` and a `log_and_report_session_state`, which
    /// then produces a fresh `PaneRenderReportWithAnsi` for the
    /// plugin — feeding back into the plugin's render loop. We
    /// therefore cache the last value sent and only re-emit when the
    /// target position would actually change.
    pub last_emitted_cursor: LastEmittedCursor,
    /// In-progress text buffer for the "+ New Session" name-entry
    /// overlay. Empty while the prompt is not open (or has been
    /// cancelled / submitted). The prompt's key handler pushes
    /// characters onto this string and the renderer draws it next to
    /// the `Name: ` label with a trailing cursor glyph. Reset to
    /// `String::new()` whenever the prompt opens and after a
    /// successful Enter submit (the buffer is `mem::take`-n into the
    /// `switch_session` argument).
    pub pending_session_name: String,
    /// Sticky scroll offset into `pending_session_name` for the New
    /// Session prompt's input row. Counts characters (not bytes)
    /// hidden behind the leading `…` indicator when the typed name is
    /// too long to fit on one row.
    ///
    /// The offset only ever *advances* in response to typing — when
    /// the cursor would otherwise spill past the right edge, the
    /// renderer pushes the offset forward enough to bring the cursor
    /// back. Backspace leaves it put, so the displayed text actually
    /// shrinks one cell per press (the cursor `_` moves left, the
    /// rightmost typed character disappears). Once the buffer is
    /// short enough to fit without truncation the renderer resets it
    /// back to 0.
    ///
    /// Without this, the truncation logic re-derived the offset on
    /// every render to "show the last N chars", which kept the
    /// displayed width constant on backspace and made the prompt look
    /// frozen.
    pub new_session_view_offset: usize,
    /// High-water-mark of the New Session prompt's content area
    /// width (everything inside the box's horizontal padding). The
    /// box never *shrinks* during a single prompt session — it grows
    /// to fit the typed name and then stays at that size while the
    /// user backspaces.
    ///
    /// Without this anchor, recentering the box after each backspace
    /// (`box_x = (cols - box_w) / 2`) flips by one column on alternate
    /// presses thanks to integer division, which cancels the cursor's
    /// leftward movement and makes backspace appear to "skip" cells
    /// — the original bug the user reported.
    ///
    /// Reset to 0 on every prompt entry (the next render then snaps
    /// it back to the default width) so an old session's expanded
    /// box does not leak into a fresh prompt.
    pub new_session_content_w: usize,
    /// Current OS soft-keyboard visibility on the attached web
    /// client, as last reported by the browser via
    /// `Event::SoftKeyboardVisibilityChanged`. Defaults to `false`
    /// so the modifier bar stays hidden until the OS keyboard is
    /// actually on screen — otherwise the bar would float above an
    /// empty bottom row on the first frame, before the user has
    /// tapped the terminal to summon the keyboard. The first
    /// visibility event after the user taps lifts this to `true`.
    /// Drives `render::render` to suppress the modifier bar when the
    /// keyboard is hidden so the bottom row of the plugin area frees
    /// up for content.
    pub soft_keyboard_visible: bool,
    /// Sticky flag: once we've auto-expanded the Sessions selector
    /// because the underlying pane is the welcome screen (session-
    /// manager in welcome mode), we never re-auto-expand. This means
    /// the user can close the selector and see the embedded welcome
    /// content if they wish; they just get the cleaner selector view
    /// by default. Reset is never needed — when the welcome session
    /// ends (user attaches / creates), the mobile plugin's whole tab
    /// is torn down and state is discarded.
    pub welcome_auto_expand_done: bool,
    /// Number of welcome-screen session cards that fit on screen the
    /// last time the renderer ran. Used by the scroll handler to cap
    /// the per-event scroll delta so the last visible card before a
    /// scroll remains visible after it — guaranteeing at least one
    /// card of overlap so the user does not lose their place.
    /// Set by `render_welcome_sessions`; consumed by
    /// `handle_selector_scroll` while in welcome mode. Defaults to 0
    /// (no cap applied) before the first welcome render.
    pub last_welcome_visible_count: usize,
    /// Fuzzy-search buffer for the welcome screen's "Session:" prompt.
    /// Empty when the prompt has no input. The render layer filters
    /// the visible session list against this string via `fuzzy_matcher`
    /// — sessions whose names fuzzy-match are kept; the rest are
    /// hidden. Cleared when the welcome flow tears down (plugin
    /// lifetime ends).
    pub welcome_search: String,
    /// Cached `SkimMatcherV2` for fuzzy matching across selectors.
    /// Lazily initialised on first keystroke (with `use_cache(true)`)
    /// so the matcher's internal score cache survives across renders.
    /// Wrapped in `Option` because `SkimMatcherV2` does not implement
    /// `Default` — matches the pattern in `default-plugins/session-
    /// manager/src/single_screen.rs`. Shared by the Sessions selector
    /// (against session names) and the Panes selector (against pane
    /// titles): only one selector is open at a time, so they never
    /// contend, and the matcher is keyed by `(haystack, needle)`
    /// internally so a pane-title cache entry cannot be mistaken for
    /// a session-name one.
    pub welcome_fuzzy_matcher: Option<SkimMatcherV2>,
    /// Fuzzy-search buffer for the Switch Pane view's "Pane:" prompt.
    /// Empty when the prompt has no input. The render layer filters
    /// the visible pane list against this string via `fuzzy_matcher`
    /// — panes whose titles fuzzy-match are kept; the rest are
    /// hidden. Cleared on `ExpandPanes` and `CollapseSelector` so
    /// each open starts on an empty prompt (mirrors `welcome_search`
    /// in the Sessions selector).
    pub panes_search: String,
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

    /// Maximum legal `viewport_v_pan` for the *current* embed bounds:
    /// the number of rows we could pan UP from the bottom-anchored
    /// default before running out of cached viewport lines. This is
    /// the same formula the renderer applies at the top of
    /// `render_embedded_viewport` to clamp a stale offset.
    ///
    /// Returns `None` when no render has happened yet (no
    /// `viewport_region` is recorded) — in that case the embed height
    /// is unknown, so callers cannot meaningfully distinguish
    /// "absorbed" from "overflowed" and should fall back to the
    /// pre-existing pure-pan behaviour. Once a single frame has
    /// rendered, `viewport_region` becomes `Some` and remains set,
    /// so this only returns `None` during the very first event tick.
    pub fn max_viewport_v_pan(&self) -> Option<usize> {
        let region = self.viewport_region?;
        let embed_height = region.row_end.saturating_sub(region.row_start);
        Some(self.current_pane_viewport_len().saturating_sub(embed_height))
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

    /// Pick the highest-scoring non-current session for the welcome
    /// screen's prompt — what `Enter` should attach to. With an empty
    /// search term, returns the alphabetically first non-current
    /// session. Returns `None` only when no non-current sessions
    /// exist.
    ///
    /// The fuzzy matcher is the same `SkimMatcherV2` instance the
    /// renderer uses, so the score it computes here matches the
    /// rendered order: the visually-topmost card is what Enter picks.
    pub fn welcome_top_match_name(&mut self) -> Option<String> {
        let search = self.welcome_search.clone();
        if search.is_empty() {
            return self
                .sessions
                .iter()
                .filter(|s| !s.is_current_session)
                .map(|s| s.name.clone())
                .min();
        }
        let matcher = self
            .welcome_fuzzy_matcher
            .get_or_insert_with(|| SkimMatcherV2::default().use_cache(true));
        let mut best: Option<(i64, String)> = None;
        for s in self.sessions.iter() {
            if s.is_current_session {
                continue;
            }
            if let Some((score, _)) = matcher.fuzzy_indices(&s.name, &search) {
                let take = match &best {
                    None => true,
                    Some((bs, bn)) => score > *bs || (score == *bs && &s.name < bn),
                };
                if take {
                    best = Some((score, s.name.clone()));
                }
            }
        }
        best.map(|(_, name)| name)
    }

    /// Pick the highest-scoring pane for the Switch Pane prompt —
    /// what `Enter` should select. With an empty search term, returns
    /// the first pane in tab/display order (which is also the first
    /// card the user sees in the unfiltered list). Returns `None`
    /// only when no panes are visible at all.
    ///
    /// The fuzzy matcher is the same `SkimMatcherV2` instance the
    /// renderer uses, so the score it computes here matches the
    /// rendered order: the visually-topmost card is what Enter picks.
    /// Matching is against pane titles (falling back to `#<id>` when
    /// the title is empty), mirroring what the renderer displays on
    /// the card's first row.
    pub fn panes_top_match(&mut self) -> Option<(usize, PaneId)> {
        let tabs: Vec<TabInfo> = self.tabs_in_order().into_iter().cloned().collect();
        let mut entries: Vec<(String, usize, PaneId)> = Vec::new();
        for tab in &tabs {
            for pane in self.panes_for_tab(tab.position) {
                let id = pane_id_of(pane);
                let title = if pane.title.is_empty() {
                    format!("#{}", pane.id)
                } else {
                    pane.title.clone()
                };
                entries.push((title, tab.position, id));
            }
        }
        if entries.is_empty() {
            return None;
        }
        let search = self.panes_search.clone();
        if search.is_empty() {
            // Tab/display-order winner — `entries` already follows
            // the order the renderer paints in.
            let first = entries.into_iter().next()?;
            return Some((first.1, first.2));
        }
        let matcher = self
            .welcome_fuzzy_matcher
            .get_or_insert_with(|| SkimMatcherV2::default().use_cache(true));
        let mut best: Option<(i64, String, usize, PaneId)> = None;
        for (title, tab_pos, pane_id) in entries.into_iter() {
            if let Some((score, _)) = matcher.fuzzy_indices(&title, &search) {
                let take = match &best {
                    None => true,
                    Some((bs, bn, _, _)) => score > *bs || (score == *bs && &title < bn),
                };
                if take {
                    best = Some((score, title, tab_pos, pane_id));
                }
            }
        }
        best.map(|(_, _, tab_pos, pane_id)| (tab_pos, pane_id))
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
    use crate::modifier_bar::CellId;

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
    /// exercise the OFF path via `clear_fit_if_active`. Both fit fields
    /// must reset to their default off-state values.
    #[test]
    fn toggle_fit_field_round_trip() {
        let mut s = State::default();
        s.fit_active = true;
        s.fit_tab_id = Some(7);
        crate::clear_fit_if_active(&mut s);
        assert!(!s.fit_active);
        assert_eq!(s.fit_tab_id, None);

        // Calling clear again while inactive is a no-op (no panic, no
        // mutation) — required because the dispatch paths invoke this
        // unconditionally on tab/pane switch.
        crate::clear_fit_if_active(&mut s);
        assert!(!s.fit_active);
        assert_eq!(s.fit_tab_id, None);
    }

    /// Build a minimal `State` whose `current_pane()` resolves to a
    /// terminal pane with `viewport_len` lines cached, and whose
    /// `viewport_region` (if `Some`) spans rows `[0, embed_height)`.
    fn state_with_viewport(
        viewport_len: usize,
        embed_height: Option<usize>,
    ) -> State {
        use zellij_tile::prelude::TabInfo;

        let mut state = State::default();

        let mut tab = TabInfo::default();
        tab.position = 0;
        state.tabs.push(tab);
        state.selected_tab_position = Some(0);

        let mut pane = PaneInfo::default();
        pane.id = 42;
        pane.is_plugin = false;
        pane.is_selectable = true;
        pane.is_suppressed = false;
        state.panes_by_tab_position.insert(0, vec![pane.clone()]);
        state.selected_pane_id = Some(PaneId::Terminal(42));

        let mut contents = PaneContents::default();
        contents.viewport = vec![String::new(); viewport_len];
        state
            .latest_pane_contents
            .insert(PaneId::Terminal(42), contents);

        if let Some(h) = embed_height {
            state.viewport_region = Some(ViewportRegion {
                row_start: 0,
                row_end: h,
                cols: 80,
                skip: 0,
                h_offset: 0,
            });
        }

        state
    }

    /// Without a recorded `viewport_region` the embed height is
    /// unknown, so the helper cannot compute a maximum and must
    /// return `None` — the contract the mouse handler relies on to
    /// fall back to pure-pan behaviour on the very first event tick.
    #[test]
    fn max_viewport_v_pan_none_without_region() {
        let state = state_with_viewport(100, None);
        assert_eq!(state.max_viewport_v_pan(), None);
    }

    /// Standard case: cached viewport is taller than the embed area,
    /// so `max_v_pan = cached - embed`.
    #[test]
    fn max_viewport_v_pan_some_typical() {
        let state = state_with_viewport(100, Some(20));
        assert_eq!(state.max_viewport_v_pan(), Some(80));
    }

    /// Embed area is taller than (or equal to) the cached viewport —
    /// no panning is possible, so the helper saturates to 0 rather
    /// than wrapping into a huge value.
    #[test]
    fn max_viewport_v_pan_saturates_when_embed_larger() {
        let state = state_with_viewport(10, Some(20));
        assert_eq!(state.max_viewport_v_pan(), Some(0));
    }

    /// Empty cache with a region still set is well-defined: 0 lines
    /// minus any embed height saturates to 0. (`pan_is_allowed` gates
    /// this case at the handler entry, but the helper must remain
    /// total.)
    #[test]
    fn max_viewport_v_pan_empty_cache() {
        let state = state_with_viewport(0, Some(20));
        assert_eq!(state.max_viewport_v_pan(), Some(0));
    }
}

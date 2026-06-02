//! The plugin's data-driven click model. Renderers emit `ClickRegion`s
//! into the shared `Frame`; a tap is resolved against them by
//! `Frame::click_to_action` and routed to the owning screen/shared
//! handler by `dispatch`. The model is screen-agnostic: every screen
//! describes its tap targets the same way, so the dispatcher never needs
//! to know which screen is active.

use zellij_tile::prelude::*;

use crate::state::State;

/// An action a click region resolves to. Each variant is owned by a
/// specific screen or shared module; `dispatch` routes it to the right
/// handler.
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
    /// embedded viewport shows that pane.
    SelectPane {
        tab_position: usize,
        pane_id: PaneId,
    },
    /// Tap on the fit (⛶) glyph in the top bar. Toggles a server-side
    /// per-tab override that resizes the focused pane's tab to match
    /// this plugin's embedded viewport area.
    ToggleFit,
    /// Tap on a cell of the modifier bar at the bottom of the plugin
    /// area. Routed through `ModifierBarController::handle_tap`.
    Keyboard(crate::modifier_bar::CellId),
    /// Tap on the hamburger glyph in the top bar. Flips the menu open
    /// state. The dropdown menu and the selectors are mutually
    /// exclusive.
    ToggleMenu,
    /// Tap on a "+ New Pane" row in the Panes selector.
    NewPaneInTab { tab_position: usize },
    /// Tap on the "+ New Tab" row at the bottom of the Panes selector.
    NewTab,
    /// Tap on the "+ New Session" row at the bottom of the Sessions
    /// selector. Opens the in-plugin name-entry prompt.
    OpenNewSessionPrompt,
    /// Tap on the "[Cancel]" button in the New Session prompt.
    CancelNewSessionPrompt,
    /// Tap on the "[Accept]" button in the New Session prompt.
    AcceptNewSessionPrompt,
    /// Tap on the "Switch to Desktop" hamburger menu row. Tears down
    /// this client's mobile tab server-side.
    ExitMobileMode,
}

/// A rectangular click target with a priority for layered scanning.
///
/// **Tight** regions (`priority == 0`) cover the visible interior of
/// a cell. They never overlap in a well-formed render and are
/// scanned first — a hit returns immediately.
///
/// **Slop** regions (`priority > 0`) cover the hit-slop halo around
/// small targets like keyboard cells. They overlap with sibling
/// slop regions on shared dividers / walls; the dispatcher resolves
/// the ambiguity by nearest-center (Euclidean squared distance from
/// the click to `center`). Ties break lex-first by `(center_y,
/// center_x)`.
#[derive(Debug, Clone)]
pub struct ClickRegion {
    /// Inclusive top row of the region in plugin coordinates.
    pub row_start: usize,
    /// Exclusive bottom row of the region in plugin coordinates.
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
    /// wins.
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
/// viewport-row / viewport-col.
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
    /// corresponds to `row_start`.
    pub skip: usize,
    /// How many leading cells were sliced off each cached viewport
    /// line when rendering.
    pub h_offset: usize,
}

/// Deterministic tiebreaker key for overlapping slop regions. Lex
/// on `(center_y, center_x)`, falling back to the region's own
/// position when no center is present (should not happen for
/// well-formed slop regions, but defensive).
pub fn slop_key(r: &ClickRegion) -> (usize, usize) {
    match r.center {
        Some((cx, cy)) => (cy, cx),
        None => (r.row_start, r.col_start),
    }
}

/// Translate a resolved `ClickAction` into the corresponding
/// shim/state mutation by routing it to the owning screen or shared
/// module. Returns whether the plugin should re-render immediately.
pub fn dispatch(state: &mut State, action: ClickAction) -> bool {
    match action {
        ClickAction::ExpandSessions => state.open_sessions(),
        ClickAction::ExpandPanes => state.open_panes(),
        ClickAction::ToggleMenu => state.menu.toggle(&mut state.active),
        ClickAction::CollapseSelector => state.collapse_selector(),
        ClickAction::SelectSession(name) => {
            state.sessions.select_session(&mut state.active, &name)
        },
        ClickAction::OpenNewSessionPrompt => state.open_new_session_prompt(),
        ClickAction::CancelNewSessionPrompt => state.cancel_new_session_prompt(),
        ClickAction::AcceptNewSessionPrompt => state.accept_new_session_prompt(),
        ClickAction::SelectPane {
            tab_position,
            pane_id,
        } => state.select_pane(tab_position, pane_id),
        ClickAction::ToggleFit => state.toggle_fit(),
        ClickAction::NewPaneInTab { tab_position } => state.new_pane_in_tab(tab_position),
        ClickAction::NewTab => state.new_tab(),
        ClickAction::ExitMobileMode => {
            exit_mobile_mode();
            true
        },
        ClickAction::Keyboard(cell) => state.keyboard_tap(cell),
    }
}

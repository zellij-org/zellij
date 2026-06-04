use zellij_tile::prelude::*;

use crate::state::State;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickAction {
    ExpandSessions,
    ExpandPanes,
    CollapseSelector,
    SelectSession(String),
    SelectPane {
        tab_position: usize,
        pane_id: PaneId,
    },
    ToggleFit,
    Keyboard(crate::components::modifier_bar::CellId),
    ToggleMenu,
    NewPaneInTab { tab_position: usize },
    NewTab,
    OpenNewSessionPrompt,
    CancelNewSessionPrompt,
    AcceptNewSessionPrompt,
    ExitMobileMode,
}

#[derive(Debug, Clone)]
pub struct ClickRegion {
    pub row_start: usize,
    pub row_end: usize,
    pub col_start: usize,
    pub col_end: usize,
    pub action: ClickAction,
    pub priority: u8,
    pub center: Option<(usize, usize)>,
}

impl ClickRegion {
    pub fn tight(
        row: usize,
        col_start: usize,
        col_end: usize,
        action: ClickAction,
    ) -> Self {
        Self::tight_range(row, row + 1, col_start, col_end, action)
    }

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

    pub fn slop(
        row: usize,
        col_start: usize,
        col_end: usize,
        action: ClickAction,
        center: (usize, usize),
    ) -> Self {
        Self::slop_range(row, row + 1, col_start, col_end, action, center)
    }

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

#[derive(Debug, Clone, Copy)]
pub struct ViewportRegion {
    pub row_start: usize,
    pub row_end: usize,
    pub cols: usize,
    pub skip: usize,
    pub h_offset: usize,
}

pub fn slop_key(r: &ClickRegion) -> (usize, usize) {
    match r.center {
        Some((cx, cy)) => (cy, cx),
        None => (r.row_start, r.col_start),
    }
}

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

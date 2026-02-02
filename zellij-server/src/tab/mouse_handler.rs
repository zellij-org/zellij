use zellij_utils::data::{Direction, Resize, ResizeStrategy};
use zellij_utils::errors::prelude::*;
use zellij_utils::input::mouse::{MouseEvent, MouseEventType};
use zellij_utils::pane_size::PaneGeom;
use zellij_utils::position::Position;

use crate::panes::PaneId;
use crate::ClientId;

use super::{Pane, Tab};

#[derive(Debug, Default, Copy, Clone)]
pub struct MouseEffect {
    pub state_changed: bool,
    pub leave_clipboard_message: bool,
    pub group_toggle: Option<PaneId>,
    pub group_add: Option<PaneId>,
    pub ungroup: bool,
}

impl MouseEffect {
    pub fn state_changed() -> Self {
        MouseEffect {
            state_changed: true,
            leave_clipboard_message: false,
            group_toggle: None,
            group_add: None,
            ungroup: false,
        }
    }
    pub fn leave_clipboard_message() -> Self {
        MouseEffect {
            state_changed: false,
            leave_clipboard_message: true,
            group_toggle: None,
            group_add: None,
            ungroup: false,
        }
    }
    pub fn state_changed_and_leave_clipboard_message() -> Self {
        MouseEffect {
            state_changed: true,
            leave_clipboard_message: true,
            group_toggle: None,
            group_add: None,
            ungroup: false,
        }
    }
    pub fn group_toggle(pane_id: PaneId) -> Self {
        MouseEffect {
            state_changed: true,
            leave_clipboard_message: false,
            group_toggle: Some(pane_id),
            group_add: None,
            ungroup: false,
        }
    }
    pub fn group_add(pane_id: PaneId) -> Self {
        MouseEffect {
            state_changed: true,
            leave_clipboard_message: false,
            group_toggle: None,
            group_add: Some(pane_id),
            ungroup: false,
        }
    }
    pub fn ungroup() -> Self {
        MouseEffect {
            state_changed: true,
            leave_clipboard_message: false,
            group_toggle: None,
            group_add: None,
            ungroup: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum MouseAction {
    GroupToggle(PaneId),
    GroupAdd(PaneId),
    Ungroup,
    StartResize {
        pane_id: PaneId,
        edge: PaneEdge,
        is_floating: bool,
        position: Position,
    },
    ContinueResize {
        position: Position,
    },
    StopResize {
        position: Position,
    },
    FocusPane {
        pane_id: PaneId,
        position: Position,
    },
    ShowFloatingPanesAndFocus {
        pane_id: PaneId,
    },
    StartSelection {
        pane_id: PaneId,
        position: Position,
    },
    UpdateSelection {
        position: Position,
    },
    EndSelection {
        position: Position,
    },
    StartMovingFloatingPane {
        position: Position,
    },
    ContinueMovingFloatingPane {
        position: Position,
    },
    StopMovingFloatingPane {
        position: Position,
    },
    ScrollUp {
        pane_id: PaneId,
        lines: usize,
    },
    ScrollDown {
        pane_id: PaneId,
        lines: usize,
    },
    UpdateHover {
        pane_id: Option<PaneId>,
    },
    SendToTerminal {
        pane_id: PaneId,
        event: MouseEvent,
    },
    FrameIntercepted {
        pane_id: PaneId,
    },
    NoAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneEdge {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

pub struct PaneResizeState {
    pub pane_id: PaneId,
    pub edge: PaneEdge,
    pub start_position: Position,
    pub start_geom: PaneGeom,
    pub is_floating: bool,
}

struct ClickedPaneDetails {
    pane_id: PaneId,
    on_frame: bool,
    frame_intercepted: bool,
    edge: Option<PaneEdge>,
    is_floating: bool,
    terminal_wants_mouse: bool,
}

struct MouseEventContext {
    pane_id_at_position: Option<PaneId>,
    active_pane_id: Option<PaneId>,
    floating_visible: bool,
    pane_being_resized: bool,
    selecting_with_mouse: bool,
    pane_being_moved: bool,
    clicked_pane: Option<ClickedPaneDetails>,
    pinned_selectable: Option<PaneId>,
    pinned_unselectable: Option<PaneId>,
}

fn edge_and_delta_to_strategies(
    edge: PaneEdge,
    delta_x: isize,
    delta_y: isize,
) -> Vec<ResizeStrategy> {
    use Direction::*;
    use Resize::*;

    match edge {
        PaneEdge::Left => {
            let resize = if delta_x < 0 { Increase } else { Decrease };
            vec![ResizeStrategy::new(resize, Some(Left))]
        },
        PaneEdge::Right => {
            let resize = if delta_x > 0 { Increase } else { Decrease };
            vec![ResizeStrategy::new(resize, Some(Right))]
        },
        PaneEdge::Top => {
            let resize = if delta_y < 0 { Increase } else { Decrease };
            vec![ResizeStrategy::new(resize, Some(Up))]
        },
        PaneEdge::Bottom => {
            let resize = if delta_y > 0 { Increase } else { Decrease };
            vec![ResizeStrategy::new(resize, Some(Down))]
        },
        PaneEdge::TopLeft => {
            let mut strategies = vec![];
            // Top edge
            let resize_y = if delta_y < 0 { Increase } else { Decrease };
            strategies.push(ResizeStrategy::new(resize_y, Some(Up)));
            // Left edge
            let resize_x = if delta_x < 0 { Increase } else { Decrease };
            strategies.push(ResizeStrategy::new(resize_x, Some(Left)));
            strategies
        },
        PaneEdge::TopRight => {
            let mut strategies = vec![];
            // Top edge
            let resize_y = if delta_y < 0 { Increase } else { Decrease };
            strategies.push(ResizeStrategy::new(resize_y, Some(Up)));
            // Right edge
            let resize_x = if delta_x > 0 { Increase } else { Decrease };
            strategies.push(ResizeStrategy::new(resize_x, Some(Right)));
            strategies
        },
        PaneEdge::BottomLeft => {
            let mut strategies = vec![];
            // Bottom edge
            let resize_y = if delta_y > 0 { Increase } else { Decrease };
            strategies.push(ResizeStrategy::new(resize_y, Some(Down)));
            // Left edge
            let resize_x = if delta_x < 0 { Increase } else { Decrease };
            strategies.push(ResizeStrategy::new(resize_x, Some(Left)));
            strategies
        },
        PaneEdge::BottomRight => {
            let mut strategies = vec![];
            // Bottom edge
            let resize_y = if delta_y > 0 { Increase } else { Decrease };
            strategies.push(ResizeStrategy::new(resize_y, Some(Down)));
            // Right edge
            let resize_x = if delta_x > 0 { Increase } else { Decrease };
            strategies.push(ResizeStrategy::new(resize_x, Some(Right)));
            strategies
        },
    }
}

pub struct MouseHandler;

impl MouseHandler {
    pub fn handle_mouse_event(
        tab: &mut Tab,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let context = Self::gather_mouse_event_context(tab, event, client_id)?;
        let action = Self::determine_mouse_action(event, &context)?;
        Self::execute_mouse_action(tab, action, event, client_id)
    }

    fn gather_mouse_event_context(
        tab: &mut Tab,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Result<MouseEventContext> {
        let err_context = || format!("failed to gather context for event {event:?}");

        let pane_id_at_position = Self::get_pane_at(tab, &event.position, false)
            .with_context(err_context)?
            .map(|p| p.pid());
        let active_pane_id = tab.get_active_pane_id(client_id);
        let floating_visible = tab.floating_panes.panes_are_visible();

        let clicked_pane = pane_id_at_position
            .and_then(|id| Self::gather_clicked_pane_details(tab, id, &event.position, active_pane_id, event, client_id));

        let (pinned_selectable, pinned_unselectable) = if !floating_visible {
            let selectable = tab.floating_panes.get_pinned_pane_id_at(&event.position, true).ok().flatten();
            let unselectable = tab.floating_panes.get_pinned_pane_id_at(&event.position, false).ok().flatten();
            (selectable, unselectable)
        } else {
            (None, None)
        };

        Ok(MouseEventContext {
            pane_id_at_position,
            active_pane_id,
            floating_visible,
            pane_being_resized: tab.pane_being_resized_with_mouse.is_some(),
            selecting_with_mouse: tab.selecting_with_mouse_in_pane.is_some(),
            pane_being_moved: tab.floating_panes.pane_is_being_moved_with_mouse(),
            clicked_pane,
            pinned_selectable,
            pinned_unselectable,
        })
    }

    fn gather_clicked_pane_details(
        tab: &mut Tab,
        pane_id: PaneId,
        position: &Position,
        active_pane_id: Option<PaneId>,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Option<ClickedPaneDetails> {
        let is_floating = tab.floating_panes.panes_contain(&pane_id);
        let pane = Self::get_pane_at(tab, position, false).ok()??;

        let on_frame = pane.position_is_on_frame(position);
        let frame_intercepted = on_frame && pane.intercept_mouse_event_on_frame(event, client_id);
        let edge = if on_frame { pane.get_edge_at_position(position) } else { None };
        let terminal_wants_mouse = if Some(pane_id) == active_pane_id {
            let relative_position = pane.relative_position(position);
            pane.mouse_left_click(&relative_position, false).is_some()
        } else {
            false
        };

        Some(ClickedPaneDetails {
            pane_id,
            on_frame,
            frame_intercepted,
            edge,
            is_floating,
            terminal_wants_mouse,
        })
    }

    fn start_pane_resize_with_mouse(
        tab: &mut Tab,
        pane_id: PaneId,
        edge: PaneEdge,
        position: Position,
        _client_id: ClientId,
    ) -> Result<()> {
        let err_context = || format!("failed to start pane resize for pane {pane_id:?}");

        // Determine if floating or tiled
        let is_floating = tab.floating_panes.panes_contain(&pane_id);

        // Get current pane geometry
        let start_geom = if is_floating {
            tab.floating_panes
                .get_pane(pane_id)
                .map(|p| p.position_and_size())
                .with_context(err_context)?
        } else {
            tab.tiled_panes
                .get_pane(pane_id)
                .map(|p| p.position_and_size())
                .with_context(err_context)?
        };

        tab.pane_being_resized_with_mouse = Some(PaneResizeState {
            pane_id,
            edge,
            start_position: position,
            start_geom,
            is_floating,
        });

        Ok(())
    }

    fn continue_pane_resize_with_mouse(
        tab: &mut Tab,
        current_position: Position,
        _client_id: ClientId,
    ) -> Result<bool> { // bool -> state changed
        let err_context = || "failed to continue pane resize with mouse";

        // Extract needed values from resize_state to avoid borrow issues
        let (pane_id, edge, is_floating, delta_x, delta_y) = if let Some(resize_state) = &tab.pane_being_resized_with_mouse {
            // Calculate delta from start_position to current_position
            let delta_x = current_position.column() as isize
                - resize_state.start_position.column() as isize;
            let delta_y = current_position.line()
                - resize_state.start_position.line();

            // Only proceed if there's a meaningful delta
            if delta_x == 0 && delta_y == 0 {
                return Ok(false);
            }

            (resize_state.pane_id, resize_state.edge, resize_state.is_floating, delta_x, delta_y)
        } else {
            return Ok(true);
        };

        // Convert edge + delta to ResizeStrategy
        let strategies = edge_and_delta_to_strategies(edge, delta_x, delta_y);

        // Apply appropriate resize function
        if is_floating {
            Self::resize_floating_pane_with_strategies(
                tab,
                pane_id,
                &strategies,
                (delta_x.unsigned_abs(), delta_y.unsigned_abs()),
            ).with_context(err_context)?;
        } else {
            let allow_inverting_strategy = false; // bad ux for mouse resize
            Self::resize_tiled_pane_with_strategies(
                tab,
                pane_id,
                &strategies,
                (delta_x.abs() as f64, delta_y.abs() as f64),
                allow_inverting_strategy,
            ).with_context(err_context)?;
        }

        // Update start_position to current position (incremental)
        if let Some(resize_state) = tab.pane_being_resized_with_mouse.as_mut() {
            resize_state.start_position = current_position;
        }

        tab.set_force_render();

        Ok(true)
    }

    fn stop_pane_resize_with_mouse(
        tab: &mut Tab,
        final_position: Position,
        client_id: ClientId,
    ) -> Result<bool> { // bool -> never_resized
        let err_context = || "failed to stop pane resize with mouse";

        // Perform final resize with any remaining delta
        let start_geom = tab.pane_being_resized_with_mouse.as_ref().map(|p| p.start_geom.clone());
        let pane_id = tab.pane_being_resized_with_mouse.as_ref().map(|p| p.pane_id);
        let _resized = Self::continue_pane_resize_with_mouse(tab, final_position, client_id)
            .with_context(err_context)?;
        let last_geom = pane_id.and_then(|pane_id| tab.get_pane_with_id(pane_id)).map(|p| p.position_and_size());
        let never_resized = match (start_geom, last_geom) {
            (Some(start_geom), Some(last_geom)) => start_geom == last_geom,
            _ => false
        };

        // Clear resize state
        tab.pane_being_resized_with_mouse = None;

        Ok(never_resized)
    }

    fn resize_floating_pane_with_strategies(
        tab: &mut Tab,
        pane_id: PaneId,
        strategies: &[ResizeStrategy],
        change_by: (usize, usize),
    ) -> Result<()> {
        let err_context = || format!("failed to resize floating pane {pane_id:?}");

        tab.floating_panes
            .resize_pane_with_strategies(pane_id, strategies, change_by)
            .with_context(err_context)?;

        tab.swap_layouts.set_is_floating_damaged();

        Ok(())
    }

    fn resize_tiled_pane_with_strategies(
        tab: &mut Tab,
        pane_id: PaneId,
        strategies: &[ResizeStrategy],
        change_by: (f64, f64),
        allow_inverting_strategy: bool,
    ) -> Result<()> {
        let err_context = || format!("failed to resize tiled pane {pane_id:?}");

        // Calculate percentage based on total viewport size for 1:1 cell mapping
        // Formula: (delta_cells / total_viewport_cells) * 100.0 = percentage
        // This ensures 1 cell of mouse movement = exactly 1 cell of pane resize
        let viewport = tab.viewport.borrow();
        let viewport_cols = viewport.cols;
        let viewport_rows = viewport.rows;

        let change_by_percent = (
            if viewport_cols > 0 {
                (change_by.0 / viewport_cols as f64) * 100.0  // cols
            } else {
                0.0
            },
            if viewport_rows > 0 {
                (change_by.1 / viewport_rows as f64) * 100.0  // rows
            } else {
                0.0
            },
        );

        tab.tiled_panes
            .resize_pane_with_strategies(pane_id, strategies, change_by_percent, allow_inverting_strategy)
            .with_context(err_context)?;

        tab.swap_layouts.set_is_tiled_damaged();

        Ok(())
    }

    fn execute_mouse_action(
        tab: &mut Tab,
        action: MouseAction,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || format!("failed to execute mouse action {action:?} for client {client_id}");

        match action {
            MouseAction::GroupToggle(pane_id) => {
                Ok(MouseEffect::group_toggle(pane_id))
            }
            MouseAction::GroupAdd(pane_id) => {
                Ok(MouseEffect::group_add(pane_id))
            }
            MouseAction::Ungroup => {
                Ok(MouseEffect::ungroup())
            }
            MouseAction::StartResize { pane_id, edge, is_floating: _, position } => {
                tab.mouse_hover_pane_id.remove(&client_id);
                Self::start_pane_resize_with_mouse(tab, pane_id, edge, position, client_id)
                    .with_context(err_context)?;
                Ok(MouseEffect::state_changed())
            }
            MouseAction::ContinueResize { position } => {
                let state_changed = Self::continue_pane_resize_with_mouse(tab, position, client_id)
                    .with_context(err_context)?;
                if state_changed {
                    Ok(MouseEffect::state_changed())
                } else {
                    Ok(MouseEffect::default())
                }
            }
            MouseAction::StopResize { position } => {
                Self::execute_stop_resize(tab, position, client_id)
            }
            MouseAction::FocusPane { pane_id: _, position } => {
                Self::execute_focus_pane(tab, position, client_id)
            }
            MouseAction::ShowFloatingPanesAndFocus { pane_id } => {
                tab.show_floating_panes();
                tab.floating_panes.focus_pane(pane_id, client_id);
                Ok(MouseEffect::state_changed())
            }
            MouseAction::StartSelection { pane_id, position } => {
                let pane = tab.get_pane_with_id_mut(pane_id)
                    .ok_or_else(|| anyhow!("Failed to find pane {pane_id:?}"))?;
                let relative_position = pane.relative_position(&position);
                let mut leave_clipboard_message = false;
                pane.start_selection(&relative_position, client_id);
                if pane.get_selected_text(client_id).is_some() {
                    leave_clipboard_message = true;
                }
                if pane.supports_mouse_selection() {
                    tab.selecting_with_mouse_in_pane = Some(pane_id);
                }
                if leave_clipboard_message {
                    Ok(MouseEffect::leave_clipboard_message())
                } else {
                    Ok(MouseEffect::default())
                }
            }
            MouseAction::UpdateSelection { position } => {
                if let Some(pane_id_with_selection) = tab.selecting_with_mouse_in_pane {
                    if let Some(pane_with_selection) = tab.get_pane_with_id_mut(pane_id_with_selection) {
                        let relative_position = pane_with_selection.relative_position(&position);
                        pane_with_selection.update_selection(&relative_position, client_id);
                    }
                }
                Ok(MouseEffect::default())
            }
            MouseAction::EndSelection { position } => {
                Self::execute_end_selection(tab, position, client_id)
            }
            MouseAction::StartMovingFloatingPane { position } => {
                Self::execute_move_floating_pane(tab, position)
            }
            MouseAction::ContinueMovingFloatingPane { position } => {
                Self::execute_move_floating_pane(tab, position)
            }
            MouseAction::StopMovingFloatingPane { position } => {
                Self::execute_stop_moving_floating_pane(tab, position, client_id)
            }
            MouseAction::ScrollUp { pane_id: _, lines } => {
                Self::handle_scrollwheel_up(tab, &event.position, lines, client_id)
                    .with_context(err_context)
            }
            MouseAction::ScrollDown { pane_id: _, lines } => {
                Self::handle_scrollwheel_down(tab, &event.position, lines, client_id)
                    .with_context(err_context)
            }
            MouseAction::UpdateHover { pane_id } => {
                Self::execute_update_hover(tab, pane_id, client_id)
            }
            MouseAction::SendToTerminal { pane_id, event } => {
                Self::execute_send_to_terminal(tab, pane_id, event, client_id)
            }
            MouseAction::FrameIntercepted { pane_id: _ } => {
                tab.set_force_render();
                Ok(MouseEffect::state_changed())
            }
            MouseAction::NoAction => {
                Ok(MouseEffect::default())
            }
        }
    }

    fn execute_stop_resize(
        tab: &mut Tab,
        position: Position,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || "failed to stop resize";
        let never_resized = Self::stop_pane_resize_with_mouse(tab, position, client_id)
            .with_context(err_context)?;
        if never_resized {
            let pane_id_at_position = Self::get_pane_at(tab, &position, false)
                .with_context(err_context)?
                .map(|p| p.pid());
            let active_pane_id = tab.get_active_pane_id(client_id)
                .ok_or_else(|| anyhow!("Failed to find active pane"))?;
            if let Some(pane_id) = pane_id_at_position {
                if pane_id != active_pane_id {
                    Self::focus_pane_at(tab, &position, client_id)
                        .with_context(err_context)?;
                }
            }
        }
        Ok(MouseEffect::state_changed())
    }

    fn execute_focus_pane(
        tab: &mut Tab,
        position: Position,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || "failed to focus pane";
        tab.mouse_hover_pane_id.remove(&client_id);
        let active_pane_id_before = tab.get_active_pane_id(client_id)
            .ok_or_else(|| anyhow!("Failed to find active pane"))?;

        Self::focus_pane_at(tab, &position, client_id)
            .with_context(err_context)?;

        if let Some(pane_at_position) = Self::unselectable_pane_at_position(tab, &position) {
            let relative_position = pane_at_position.relative_position(&position);
            pane_at_position.start_selection(&relative_position, client_id);
        }

        if tab.floating_panes.panes_are_visible() {
            let search_selectable = false;
            let moved_pane_with_mouse = tab.floating_panes
                .move_pane_with_mouse(position, search_selectable);
            let active_pane_id_after = tab.get_active_pane_id(client_id)
                .ok_or_else(|| anyhow!("Failed to find active pane"))?;
            if moved_pane_with_mouse || active_pane_id_before != active_pane_id_after {
                return Ok(MouseEffect::state_changed());
            } else {
                return Ok(MouseEffect::default());
            }
        }

        let active_pane_id_after = tab.get_active_pane_id(client_id)
            .ok_or_else(|| anyhow!("Failed to find active pane"))?;
        if active_pane_id_before != active_pane_id_after {
            Ok(MouseEffect::state_changed())
        } else {
            Ok(MouseEffect::default())
        }
    }

    fn execute_end_selection(
        tab: &mut Tab,
        position: Position,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || "failed to end selection";
        let mut leave_clipboard_message = false;
        let copy_on_release = tab.copy_on_select;

        if let Some(pane_with_selection) = tab.selecting_with_mouse_in_pane
            .and_then(|p_id| tab.get_pane_with_id_mut(p_id))
        {
            let mut relative_position = pane_with_selection.relative_position(&position);

            relative_position.change_column(
                (relative_position.column())
                    .max(0)
                    .min(pane_with_selection.get_content_columns()),
            );

            relative_position.change_line(
                (relative_position.line())
                    .max(0)
                    .min(pane_with_selection.get_content_rows() as isize),
            );

            if let Some(mouse_event) = pane_with_selection.mouse_left_click_release(&relative_position) {
                tab.write_to_active_terminal(&None, mouse_event.into_bytes(), false, client_id)
                    .with_context(err_context)?;
            } else {
                let relative_position = pane_with_selection.relative_position(&position);
                pane_with_selection.end_selection(&relative_position, client_id);
                if pane_with_selection.supports_mouse_selection() {
                    if copy_on_release {
                        let selected_text = pane_with_selection.get_selected_text(client_id);
                        if let Some(selected_text) = selected_text {
                            leave_clipboard_message = true;
                            tab.write_selection_to_clipboard(&selected_text)
                                .with_context(err_context)?;
                        }
                    }
                }
                tab.selecting_with_mouse_in_pane = None;
            }
        }

        if leave_clipboard_message {
            Ok(MouseEffect::leave_clipboard_message())
        } else {
            Ok(MouseEffect::default())
        }
    }

    fn execute_move_floating_pane(tab: &mut Tab, position: Position) -> Result<MouseEffect> {
        let search_selectable = false;
        if tab.floating_panes.move_pane_with_mouse(position, search_selectable) {
            tab.swap_layouts.set_is_floating_damaged();
            tab.set_force_render();
            Ok(MouseEffect::state_changed())
        } else {
            Ok(MouseEffect::default())
        }
    }

    fn execute_stop_moving_floating_pane(
        tab: &mut Tab,
        position: Position,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || "failed to stop moving floating pane";
        let never_moved = tab.floating_panes.stop_moving_pane_with_mouse(position);
        if never_moved {
            let active_pane_id = tab.get_active_pane_id(client_id)
                .ok_or_else(|| anyhow!("Failed to find active pane"))?;
            let pane_id_at_position = Self::get_pane_at(tab, &position, false)
                .with_context(err_context)?
                .ok_or_else(|| anyhow!("Failed to find pane at position"))?
                .pid();
            if active_pane_id != pane_id_at_position {
                Self::focus_pane_at(tab, &position, client_id)
                    .with_context(err_context)?;
            }
        }
        Ok(MouseEffect::default())
    }

    fn execute_update_hover(
        tab: &mut Tab,
        pane_id: Option<PaneId>,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let mut should_render = false;
        match pane_id {
            Some(pid) => {
                if let Some(pane) = tab.get_pane_with_id(pid) {
                    let pane_is_selectable = pane.selectable();
                    if tab.advanced_mouse_actions && pane_is_selectable {
                        tab.mouse_hover_pane_id.insert(client_id, pid);
                    } else if tab.advanced_mouse_actions {
                        tab.mouse_hover_pane_id.remove(&client_id);
                    }
                    should_render = true;
                }
            }
            None => {
                let removed = tab.mouse_hover_pane_id.remove(&client_id);
                if removed.is_some() {
                    should_render = true;
                }
            }
        }
        let mut mouse_effect = if should_render {
            MouseEffect::state_changed()
        } else {
            MouseEffect::default()
        };
        mouse_effect.leave_clipboard_message = true;
        Ok(mouse_effect)
    }

    fn execute_send_to_terminal(
        tab: &mut Tab,
        pane_id: PaneId,
        event: MouseEvent,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || format!("failed to send to terminal for pane {pane_id:?}");
        let mut should_render = false;
        let active_pane_id = tab.get_active_pane_id(client_id)
            .ok_or_else(|| anyhow!("Failed to find active pane"))?;
        if pane_id == active_pane_id {
            let pane = tab.get_pane_with_id(pane_id)
                .ok_or_else(|| anyhow!("Failed to find pane {pane_id:?}"))?;
            let relative_position = pane.relative_position(&event.position);
            let mut event_for_pane = event.clone();
            event_for_pane.position = relative_position;
            if let Some(mouse_event) = pane.mouse_event(&event_for_pane, client_id) {
                if !pane.position_is_on_frame(&event.position) {
                    tab.write_to_active_terminal(&None, mouse_event.into_bytes(), false, client_id)
                        .with_context(err_context)?;
                }
            }
            let removed_hover = tab.mouse_hover_pane_id.remove(&client_id);
            if removed_hover.is_some() {
                should_render = true;
            }
        }
        let mouse_effect = if should_render {
            MouseEffect::state_changed()
        } else {
            MouseEffect::default()
        };
        Ok(mouse_effect)
    }

    fn determine_mouse_action(
        event: &MouseEvent,
        ctx: &MouseEventContext,
    ) -> Result<MouseAction> {
        if ctx.pane_being_resized {
            return Ok(match event.event_type {
                MouseEventType::Motion => MouseAction::ContinueResize { position: event.position },
                MouseEventType::Release => MouseAction::StopResize { position: event.position },
                _ => MouseAction::NoAction,
            });
        }

        if ctx.selecting_with_mouse {
            return Ok(match event.event_type {
                MouseEventType::Motion if event.left => MouseAction::UpdateSelection { position: event.position },
                MouseEventType::Release if event.left => MouseAction::EndSelection { position: event.position },
                _ => MouseAction::NoAction,
            });
        }

        if ctx.pane_being_moved {
            return Ok(match event.event_type {
                MouseEventType::Motion if event.left => MouseAction::ContinueMovingFloatingPane { position: event.position },
                MouseEventType::Release if event.left => MouseAction::StopMovingFloatingPane { position: event.position },
                _ => MouseAction::NoAction,
            });
        }

        if event.alt {
            let is_left_press = event.left && event.event_type == MouseEventType::Press;
            let is_left_motion = event.left && event.event_type == MouseEventType::Motion;

            if is_left_press {
                if let Some(pane_id) = ctx.pane_id_at_position {
                    return Ok(MouseAction::GroupToggle(pane_id));
                }
            }
            if is_left_motion {
                if let Some(pane_id) = ctx.pane_id_at_position {
                    return Ok(MouseAction::GroupAdd(pane_id));
                }
            }
            if event.right {
                return Ok(MouseAction::Ungroup);
            }
            return Ok(MouseAction::NoAction);
        }

        if event.wheel_up || event.wheel_down {
            if let Some(pane_id) = ctx.pane_id_at_position {
                if event.wheel_up {
                    return Ok(MouseAction::ScrollUp { pane_id, lines: 3 });
                }
                if event.wheel_down {
                    return Ok(MouseAction::ScrollDown { pane_id, lines: 3 });
                }
            }
            return Ok(MouseAction::NoAction);
        }

        let is_ctrl_left_press = event.ctrl && event.left && event.event_type == MouseEventType::Press;
        if is_ctrl_left_press {
            let Some(details) = &ctx.clicked_pane else {
                return Ok(MouseAction::NoAction);
            };
            if details.on_frame {
                if details.frame_intercepted {
                    return Ok(MouseAction::FrameIntercepted { pane_id: details.pane_id });
                }
                if let Some(edge) = details.edge {
                    return Ok(MouseAction::StartResize {
                        pane_id: details.pane_id,
                        edge,
                        is_floating: details.is_floating,
                        position: event.position,
                    });
                }
            }
            return Ok(MouseAction::NoAction);
        }

        let is_plain_left_press = event.left && event.event_type == MouseEventType::Press && !event.ctrl && !event.alt;
        if is_plain_left_press {
            let Some(details) = &ctx.clicked_pane else {
                return Ok(MouseAction::NoAction);
            };

            let is_active_pane = Some(details.pane_id) == ctx.active_pane_id;
            let is_pinned_pane = ctx.pinned_selectable.map(|id| id == details.pane_id).unwrap_or(false);

            if details.on_frame {
                if details.frame_intercepted {
                    return Ok(MouseAction::FrameIntercepted { pane_id: details.pane_id });
                }

                let should_start_moving = ctx.floating_visible || is_pinned_pane;
                if should_start_moving {
                    return Ok(MouseAction::StartMovingFloatingPane { position: event.position });
                }

                if let Some(edge) = details.edge {
                    return Ok(MouseAction::StartResize {
                        pane_id: details.pane_id,
                        edge,
                        is_floating: false,
                        position: event.position,
                    });
                }
            }

            if is_active_pane {
                if details.terminal_wants_mouse {
                    return Ok(MouseAction::SendToTerminal { pane_id: details.pane_id, event: *event });
                } else {
                    return Ok(MouseAction::StartSelection { pane_id: details.pane_id, position: event.position });
                }
            }

            if !ctx.floating_visible {
                if let Some(pinned_id) = ctx.pinned_selectable {
                    return Ok(MouseAction::ShowFloatingPanesAndFocus { pane_id: pinned_id });
                }
                if ctx.pinned_unselectable.is_some() {
                    return Ok(MouseAction::NoAction);
                }
            }

            return Ok(MouseAction::FocusPane { pane_id: details.pane_id, position: event.position });
        }

        if event.right {
            let Some(pane_id) = ctx.pane_id_at_position else {
                return Ok(MouseAction::NoAction);
            };
            let is_active_pane = Some(pane_id) == ctx.active_pane_id;
            if is_active_pane {
                return Ok(MouseAction::SendToTerminal { pane_id, event: *event });
            }
            return Ok(MouseAction::NoAction);
        }

        if event.middle {
            let Some(pane_id) = ctx.pane_id_at_position else {
                return Ok(MouseAction::NoAction);
            };
            let is_active_pane = Some(pane_id) == ctx.active_pane_id;
            if is_active_pane {
                return Ok(MouseAction::SendToTerminal { pane_id, event: *event });
            }
            return Ok(MouseAction::NoAction);
        }

        let is_left_motion_or_release = event.left && (event.event_type == MouseEventType::Motion || event.event_type == MouseEventType::Release);
        if is_left_motion_or_release {
            let Some(details) = &ctx.clicked_pane else {
                return Ok(MouseAction::NoAction);
            };
            let is_active_pane = Some(details.pane_id) == ctx.active_pane_id;
            if is_active_pane && details.terminal_wants_mouse {
                return Ok(MouseAction::SendToTerminal { pane_id: details.pane_id, event: *event });
            }
            return Ok(MouseAction::NoAction);
        }

        let is_buttonless_motion = event.event_type == MouseEventType::Motion && !event.left && !event.right && !event.middle;
        if is_buttonless_motion {
            let Some(pane_id) = ctx.pane_id_at_position else {
                return Ok(MouseAction::UpdateHover { pane_id: None });
            };
            let is_active_pane = Some(pane_id) == ctx.active_pane_id;
            if is_active_pane {
                return Ok(MouseAction::SendToTerminal { pane_id, event: *event });
            }
            return Ok(MouseAction::UpdateHover { pane_id: Some(pane_id) });
        }

        Ok(MouseAction::NoAction)
    }

    fn unselectable_pane_at_position<'a>(tab: &'a mut Tab, point: &Position) -> Option<&'a mut Box<dyn Pane>> {
        // the repetition in this function is to appease the borrow checker, I don't like it either
        let floating_panes_are_visible = tab.floating_panes.panes_are_visible();
        if floating_panes_are_visible {
            if let Ok(Some(clicked_pane_id)) = tab.floating_panes.get_pane_id_at(point, true) {
                if let Some(pane) = tab.floating_panes.get_pane_mut(clicked_pane_id) {
                    if !pane.selectable() {
                        return Some(pane);
                    }
                }
            } else if let Ok(Some(clicked_pane_id)) = tab.get_pane_id_at(point, false) {
                if let Some(pane) = tab.tiled_panes.get_pane_mut(clicked_pane_id) {
                    if !pane.selectable() {
                        return Some(pane);
                    }
                }
            }
        } else if let Ok(Some(clicked_pane_id)) = tab.get_pane_id_at(point, false) {
            if let Some(pane) = tab.tiled_panes.get_pane_mut(clicked_pane_id) {
                if !pane.selectable() {
                    return Some(pane);
                }
            }
        }
        None
    }

    fn focus_pane_at(tab: &mut Tab, point: &Position, client_id: ClientId) -> Result<()> {
        let err_context =
            || format!("failed to focus pane at position {point:?} for client {client_id}");

        if tab.floating_panes.panes_are_visible() {
            if let Some(clicked_pane) = tab
                .floating_panes
                .get_pane_id_at(point, true)
                .with_context(err_context)?
            {
                tab.floating_panes.focus_pane(clicked_pane, client_id);
                tab.set_pane_active_at(clicked_pane);
                return Ok(());
            }
        }
        if let Some(clicked_pane) = tab.get_pane_id_at(point, true).with_context(err_context)? {
            tab.tiled_panes.focus_pane(clicked_pane, client_id);
            tab.set_pane_active_at(clicked_pane);
            if tab.floating_panes.panes_are_visible() {
                tab.hide_floating_panes();
                tab.set_force_render();
            }
        }
        Ok(())
    }

    pub fn handle_scrollwheel_up(
        tab: &mut Tab,
        point: &Position,
        lines: usize,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || {
            format!("failed to handle scrollwheel up at position {point:?} for client {client_id}")
        };

        if let Some(pane) = Self::get_pane_at(tab, point, false).with_context(err_context)? {
            let relative_position = pane.relative_position(point);
            if let Some(mouse_event) = pane.mouse_scroll_up(&relative_position) {
                tab.write_to_terminal_at(mouse_event.into_bytes(), point, client_id)
                    .with_context(err_context)?;
            } else if pane.is_alternate_mode_active() {
                // faux scrolling, send UP n times
                // do n separate writes to make sure the sequence gets adjusted for cursor keys mode
                for _ in 0..lines {
                    tab.write_to_terminal_at("\u{1b}[A".as_bytes().to_owned(), point, client_id)
                        .with_context(err_context)?;
                }
            } else {
                pane.scroll_up(lines, client_id);
            }
        }
        Ok(MouseEffect::default())
    }

    pub fn handle_scrollwheel_down(
        tab: &mut Tab,
        point: &Position,
        lines: usize,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || {
            format!(
                "failed to handle scrollwheel down at position {point:?} for client {client_id}"
            )
        };

        if let Some(pane) = Self::get_pane_at(tab, point, false).with_context(err_context)? {
            let relative_position = pane.relative_position(point);
            if let Some(mouse_event) = pane.mouse_scroll_down(&relative_position) {
                tab.write_to_terminal_at(mouse_event.into_bytes(), point, client_id)
                    .with_context(err_context)?;
            } else if pane.is_alternate_mode_active() {
                // faux scrolling, send DOWN n times
                // do n separate writes to make sure the sequence gets adjusted for cursor keys mode
                for _ in 0..lines {
                    tab.write_to_terminal_at("\u{1b}[B".as_bytes().to_owned(), point, client_id)
                        .with_context(err_context)?;
                }
            } else {
                pane.scroll_down(lines, client_id);
                if !pane.is_scrolled() {
                    if let PaneId::Terminal(pid) = pane.pid() {
                        tab.process_pending_vte_events(pid)
                            .with_context(err_context)?;
                    }
                }
            }
        }
        Ok(MouseEffect::default())
    }

    fn get_pane_at<'a>(
        tab: &'a mut Tab,
        point: &Position,
        search_selectable: bool,
    ) -> Result<Option<&'a mut Box<dyn Pane>>> {
        let err_context = || format!("failed to get pane at position {point:?}");

        if tab.floating_panes.panes_are_visible() {
            if let Some(pane_id) = tab
                .floating_panes
                .get_pane_id_at(point, search_selectable)
                .with_context(err_context)?
            {
                return Ok(tab.floating_panes.get_pane_mut(pane_id));
            }
        } else if tab.floating_panes.has_pinned_panes() {
            if let Some(pane_id) = tab
                .floating_panes
                .get_pinned_pane_id_at(point, search_selectable)
                .with_context(err_context)?
            {
                return Ok(tab.floating_panes.get_pane_mut(pane_id));
            }
        }
        if let Some(pane_id) = tab
            .get_pane_id_at(point, search_selectable)
            .with_context(err_context)?
        {
            Ok(tab.tiled_panes.get_pane_mut(pane_id))
        } else {
            Ok(None)
        }
    }

    pub fn set_mouse_selection_support(tab: &mut Tab, pane_id: PaneId, selection_support: bool) {
        if let Some(pane) = tab.get_pane_with_id_mut(pane_id) {
            pane.set_mouse_selection_support(selection_support);
        }
    }
}

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;
use zellij_utils::data::{
    ContextMenuKind, Direction, InputMode, KeyModifier, MouseButton, Resize, ResizeStrategy,
};
use zellij_utils::errors::prelude::*;
use zellij_utils::input::actions::Action;
use zellij_utils::input::mouse::{MouseEvent, MouseEventType};
use zellij_utils::input::mousebinds::{
    mouse_button_of_event, mouse_modifiers_of_event, MouseBehaviour, MouseBinding,
    MouseBindingKind, Mousebinds, PromptDirection, ResizeScrollDirection,
};
use zellij_utils::pane_size::PaneGeom;
use zellij_utils::position::Position;

use crate::background_jobs::BackgroundJob;
use crate::panes::PaneId;
use crate::plugins::PluginInstruction;
use crate::screen::{GuestModalOutcome, ScreenInstruction};
use crate::ClientId;

use super::{Pane, Tab};

fn clear_hover_for_client(tab: &mut Tab, client_id: ClientId) -> bool {
    clear_hover_for_client_keeping_plugin(tab, client_id, None)
}

fn clear_hover_for_client_keeping_plugin(
    tab: &mut Tab,
    client_id: ClientId,
    plugin_to_keep: Option<PaneId>,
) -> bool {
    let mut cleared = false;
    if let Some(prev_pid) = tab.mouse_hover_pane_id.remove(&client_id) {
        if let Some(pane) = tab.get_pane_with_id_mut(prev_pid) {
            pane.set_hover_position(None);
        }
        cleared = true;
    }
    let keeps_plugin_hover = plugin_to_keep.is_some()
        && tab.plugin_hover_pane_id.get(&client_id).copied() == plugin_to_keep;
    if keeps_plugin_hover {
        return cleared;
    }
    if let Some(prev_plugin_pid) = tab.plugin_hover_pane_id.remove(&client_id) {
        if let Some(pane) = tab.get_pane_with_id(prev_plugin_pid) {
            let _ = pane.mouse_event(
                &MouseEvent::new_buttonless_motion(Position::new(0, u16::MAX)),
                client_id,
            );
        }
        cleared = true;
    }
    cleared
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextMenuRequest {
    pub kind: ContextMenuKind,
    pub pane_id: PaneId,
    pub is_floating: bool,
    pub position: Position,
}

#[derive(Debug, Default, Clone)]
pub struct MouseEffect {
    pub state_changed: bool,
    pub leave_clipboard_message: bool,
    pub group_toggle: Option<PaneId>,
    pub group_add: Option<PaneId>,
    pub ungroup: bool,
    pub open_context_menu: Option<ContextMenuRequest>,
    pub run_actions: Option<Vec<Action>>,
}

const CLICK_TIME_THRESHOLD_MS: u128 = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MousePressOwner {
    App(PaneId),
    Group,
    Swallow,
}

#[derive(Debug, Clone)]
pub struct MouseClickRecord {
    button: MouseButton,
    modifiers: BTreeSet<KeyModifier>,
    position: Position,
    time: Instant,
    count: u8,
}

impl MouseEffect {
    pub fn state_changed() -> Self {
        MouseEffect {
            state_changed: true,
            ..Default::default()
        }
    }
    pub fn leave_clipboard_message() -> Self {
        MouseEffect {
            leave_clipboard_message: true,
            ..Default::default()
        }
    }
    pub fn state_changed_and_leave_clipboard_message() -> Self {
        MouseEffect {
            state_changed: true,
            leave_clipboard_message: true,
            ..Default::default()
        }
    }
    pub fn group_toggle(pane_id: PaneId) -> Self {
        MouseEffect {
            state_changed: true,
            group_toggle: Some(pane_id),
            ..Default::default()
        }
    }
    pub fn group_add(pane_id: PaneId) -> Self {
        MouseEffect {
            state_changed: true,
            group_add: Some(pane_id),
            ..Default::default()
        }
    }
    pub fn ungroup() -> Self {
        MouseEffect {
            state_changed: true,
            ungroup: true,
            ..Default::default()
        }
    }
    pub fn open_context_menu(request: ContextMenuRequest) -> Self {
        MouseEffect {
            open_context_menu: Some(request),
            ..Default::default()
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
    FocusPaneAndClickThrough {
        pane_id: PaneId,
        position: Position,
        event: MouseEvent,
    },
    ShowFloatingPanesAndFocus {
        pane_id: PaneId,
    },
    StartSelection {
        pane_id: PaneId,
        position: Position,
        modifiers: std::collections::BTreeSet<zellij_utils::data::KeyModifier>,
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
        app_first: bool,
    },
    ScrollDown {
        pane_id: PaneId,
        lines: usize,
        app_first: bool,
    },
    ScrollLeft {
        pane_id: PaneId,
        cols: usize,
        app_first: bool,
    },
    ScrollRight {
        pane_id: PaneId,
        cols: usize,
        app_first: bool,
    },
    PassWheelToApp {
        pane_id: PaneId,
        button: MouseButton,
    },
    ToggleFullscreen {
        pane_id: PaneId,
    },
    RunActions {
        pane_id: PaneId,
        actions: Vec<Action>,
    },
    ResizeScrollUp {
        pane_id: PaneId,
    },
    ResizeScrollDown {
        pane_id: PaneId,
    },
    ScrollToPreviousPrompt {
        pane_id: PaneId,
        app_first: bool,
    },
    ScrollToNextPrompt {
        pane_id: PaneId,
        app_first: bool,
    },
    UpdateHover {
        pane_id: Option<PaneId>,
        position: Option<Position>,
    },
    FocusOnHover {
        pane_id: PaneId,
        position: Position,
    },
    SendToTerminal {
        pane_id: PaneId,
        event: MouseEvent,
    },
    FrameIntercepted {
        pane_id: PaneId,
    },
    OpenContextMenu {
        kind: ContextMenuKind,
        pane_id: PaneId,
        is_floating: bool,
        position: Position,
    },
    ForwardRightClickToPlugin {
        pane_id: PaneId,
        position: Position,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneResizeState {
    pub pane_id: PaneId,
    pub edge: PaneEdge,
    pub start_position: Position,
    pub start_geom: PaneGeom,
    pub is_floating: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ClickedPaneDetails {
    pane_id: PaneId,
    on_frame: bool,
    frame_intercepted: bool,
    edge: Option<PaneEdge>,
    is_floating: bool,
    terminal_wants_mouse: bool,
    is_unselectable_plugin: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct MouseEventContext {
    pane_id_at_position: Option<PaneId>,
    active_pane_id: Option<PaneId>,
    floating_visible: bool,
    pane_being_resized: bool,
    selecting_with_mouse: bool,
    pane_being_moved: bool,
    clicked_pane: Option<ClickedPaneDetails>,
    advanced_mouse_actions: bool,
    pinned_selectable: Option<PaneId>,
    pinned_unselectable: Option<PaneId>,
    focus_follows_mouse: bool,
    mouse_click_through: bool,
    mouse_scroll_resize: bool,
    passthrough_pane_id: Option<PaneId>,
    context_menu_enabled: bool,
    mousebinds: Arc<Mousebinds>,
    input_mode: InputMode,
    click_count: u8,
    press_owner: Option<MousePressOwner>,
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
            vec![ResizeStrategy {
                resize,
                direction: Some(Left),
                invert_on_boundaries: false,
            }]
        },
        PaneEdge::Right => {
            let resize = if delta_x > 0 { Increase } else { Decrease };
            vec![ResizeStrategy {
                resize,
                direction: Some(Right),
                invert_on_boundaries: false,
            }]
        },
        PaneEdge::Top => {
            let resize = if delta_y < 0 { Increase } else { Decrease };
            vec![ResizeStrategy {
                resize,
                direction: Some(Up),
                invert_on_boundaries: false,
            }]
        },
        PaneEdge::Bottom => {
            let resize = if delta_y > 0 { Increase } else { Decrease };
            vec![ResizeStrategy {
                resize,
                direction: Some(Down),
                invert_on_boundaries: false,
            }]
        },
        PaneEdge::TopLeft => {
            let mut strategies = vec![];
            let resize_y = if delta_y < 0 { Increase } else { Decrease };
            strategies.push(ResizeStrategy {
                resize: resize_y,
                direction: Some(Up),
                invert_on_boundaries: false,
            });
            let resize_x = if delta_x < 0 { Increase } else { Decrease };
            strategies.push(ResizeStrategy {
                resize: resize_x,
                direction: Some(Left),
                invert_on_boundaries: false,
            });
            strategies
        },
        PaneEdge::TopRight => {
            let mut strategies = vec![];
            let resize_y = if delta_y < 0 { Increase } else { Decrease };
            strategies.push(ResizeStrategy {
                resize: resize_y,
                direction: Some(Up),
                invert_on_boundaries: false,
            });
            let resize_x = if delta_x > 0 { Increase } else { Decrease };
            strategies.push(ResizeStrategy {
                resize: resize_x,
                direction: Some(Right),
                invert_on_boundaries: false,
            });
            strategies
        },
        PaneEdge::BottomLeft => {
            let mut strategies = vec![];
            let resize_y = if delta_y > 0 { Increase } else { Decrease };
            strategies.push(ResizeStrategy {
                resize: resize_y,
                direction: Some(Down),
                invert_on_boundaries: false,
            });
            let resize_x = if delta_x < 0 { Increase } else { Decrease };
            strategies.push(ResizeStrategy {
                resize: resize_x,
                direction: Some(Left),
                invert_on_boundaries: false,
            });
            strategies
        },
        PaneEdge::BottomRight => {
            let mut strategies = vec![];
            let resize_y = if delta_y > 0 { Increase } else { Decrease };
            strategies.push(ResizeStrategy {
                resize: resize_y,
                direction: Some(Down),
                invert_on_boundaries: false,
            });
            let resize_x = if delta_x > 0 { Increase } else { Decrease };
            strategies.push(ResizeStrategy {
                resize: resize_x,
                direction: Some(Right),
                invert_on_boundaries: false,
            });
            strategies
        },
    }
}

fn next_click_count(
    previous: Option<&MouseClickRecord>,
    button: MouseButton,
    modifiers: &BTreeSet<KeyModifier>,
    position: Position,
    now: Instant,
) -> u8 {
    match previous {
        Some(previous)
            if previous.button == button
                && &previous.modifiers == modifiers
                && previous.position == position
                && now.saturating_duration_since(previous.time).as_millis()
                    <= CLICK_TIME_THRESHOLD_MS
                && previous.count < 3 =>
        {
            previous.count + 1
        },
        _ => 1,
    }
}

pub struct MouseHandler;

impl MouseHandler {
    pub(crate) fn handle_mouse_event(
        tab: &mut Tab,
        event: &MouseEvent,
        client_id: ClientId,
        passthrough_pane_id: Option<PaneId>,
        mousebinds: Arc<Mousebinds>,
    ) -> Result<MouseEffect> {
        if let Some(effect) = Self::intercept_guest_modal_mouse_event(tab, event, client_id)? {
            return Ok(effect);
        }
        let click_count = Self::count_click(tab, event, client_id);
        let mut context =
            Self::gather_mouse_event_context(tab, event, client_id, passthrough_pane_id)?;
        context.mousebinds = mousebinds;
        context.input_mode = tab.get_client_input_mode(client_id).unwrap_or_default();
        context.click_count = click_count;
        context.press_owner = tab.mouse_press_owner.get(&client_id).copied();
        let action = Self::determine_mouse_action(event, &context)?;
        Self::record_press_owner(tab, event, &action, client_id);
        let double_clicked_plugin = Self::double_clicked_plugin(event, &action, &context);
        let effect = Self::execute_mouse_action(tab, action, event, client_id)?;
        if let Some(pane_id) = double_clicked_plugin {
            if let Some(pane) = tab.get_pane_with_id_mut(pane_id) {
                let relative_position = pane.relative_position(&event.position);
                pane.mouse_double_click(&relative_position, client_id);
            }
        }
        Ok(effect)
    }

    fn count_click(tab: &mut Tab, event: &MouseEvent, client_id: ClientId) -> u8 {
        if event.event_type != MouseEventType::Press {
            return 1;
        }
        let Some(button) = mouse_button_of_event(event) else {
            return 1;
        };
        if button.is_wheel() {
            return 1;
        }
        let modifiers = mouse_modifiers_of_event(event);
        let now = Instant::now();
        let count = next_click_count(
            tab.mouse_click_tracker.get(&client_id),
            button,
            &modifiers,
            event.position,
            now,
        );
        tab.mouse_click_tracker.insert(
            client_id,
            MouseClickRecord {
                button,
                modifiers,
                position: event.position,
                time: now,
                count,
            },
        );
        count
    }

    fn record_press_owner(
        tab: &mut Tab,
        event: &MouseEvent,
        action: &MouseAction,
        client_id: ClientId,
    ) {
        let is_button_press = event.event_type == MouseEventType::Press
            && (event.left || event.right || event.middle)
            && !(event.wheel_up || event.wheel_down || event.wheel_left || event.wheel_right);
        if is_button_press {
            let owner = match action {
                MouseAction::SendToTerminal { pane_id, .. }
                | MouseAction::FocusPaneAndClickThrough { pane_id, .. } => {
                    Some(MousePressOwner::App(*pane_id))
                },
                MouseAction::GroupToggle(_) => Some(MousePressOwner::Group),
                MouseAction::StartSelection { .. }
                | MouseAction::StartResize { .. }
                | MouseAction::StartMovingFloatingPane { .. }
                | MouseAction::FocusPane { .. }
                | MouseAction::ShowFloatingPanesAndFocus { .. }
                | MouseAction::ContinueResize { .. }
                | MouseAction::StopResize { .. }
                | MouseAction::UpdateSelection { .. }
                | MouseAction::EndSelection { .. }
                | MouseAction::ContinueMovingFloatingPane { .. }
                | MouseAction::StopMovingFloatingPane { .. } => None,
                _ => Some(MousePressOwner::Swallow),
            };
            match owner {
                Some(owner) => {
                    tab.mouse_press_owner.insert(client_id, owner);
                },
                None => {
                    tab.mouse_press_owner.remove(&client_id);
                },
            }
        } else if event.event_type == MouseEventType::Release {
            tab.mouse_press_owner.remove(&client_id);
        }
    }

    fn double_clicked_plugin(
        event: &MouseEvent,
        action: &MouseAction,
        ctx: &MouseEventContext,
    ) -> Option<PaneId> {
        if ctx.click_count != 2
            || event.event_type != MouseEventType::Press
            || !event.left
            || mouse_button_of_event(event) != Some(MouseButton::Left)
        {
            return None;
        }
        let pane_id = ctx.pane_id_at_position?;
        if !matches!(pane_id, PaneId::Plugin(_)) {
            return None;
        }
        match action {
            MouseAction::StartSelection { pane_id: id, .. } if *id == pane_id => Some(pane_id),
            MouseAction::FocusPane { pane_id: id, .. }
            | MouseAction::FocusPaneAndClickThrough { pane_id: id, .. }
                if *id == pane_id =>
            {
                Some(pane_id)
            },
            _ => None,
        }
    }

    fn intercept_guest_modal_mouse_event(
        tab: &mut Tab,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Result<Option<MouseEffect>> {
        let pane_id_at_position = Self::get_pane_at(tab, &event.position, false)?.map(|p| p.pid());
        let pane_id = match pane_id_at_position {
            Some(pane_id) => pane_id,
            None => return Ok(None),
        };
        if !tab.pane_has_guest_modal_for_client(pane_id, client_id) {
            return Ok(None);
        }
        let hit_option = if event.event_type == MouseEventType::Release && event.left {
            let style = tab.style;
            if let Some(pane) = tab.get_pane_with_id(pane_id) {
                let relative_position = pane.relative_position(&event.position);
                let rows = pane.get_content_rows();
                let columns = pane.get_content_columns();
                let row = relative_position.line();
                if row < 0 {
                    None
                } else {
                    let session_name = pane.guest_session_name().unwrap_or_default();
                    let selection = pane.guest_modal_selection(client_id).unwrap_or(0);
                    let shortcuts = pane.guest_modal_shortcuts();
                    crate::panes::nested_session_modal::guest_modal_option_at_content_row(
                        rows,
                        columns,
                        row as usize,
                        &style,
                        &session_name,
                        selection,
                        &shortcuts,
                    )
                }
            } else {
                None
            }
        } else {
            None
        };
        if let Some(option) = hit_option {
            let outcome = match option {
                0 => GuestModalOutcome::Zoom,
                _ => GuestModalOutcome::Descend,
            };
            let _ = tab
                .senders
                .send_to_screen(ScreenInstruction::GuestModalChoice {
                    client_id,
                    pane_id,
                    outcome,
                });
        }
        Ok(Some(MouseEffect::state_changed()))
    }

    fn gather_mouse_event_context(
        tab: &mut Tab,
        event: &MouseEvent,
        client_id: ClientId,
        passthrough_pane_id: Option<PaneId>,
    ) -> Result<MouseEventContext> {
        let err_context = || format!("failed to gather context for event {event:?}");

        Self::forget_selection_in_missing_pane(tab);
        let pane_id_at_position = Self::get_pane_at(tab, &event.position, false)
            .with_context(err_context)?
            .map(|p| p.pid());
        let active_pane_id = tab.get_active_pane_id(client_id);
        let floating_visible = tab.floating_panes.panes_are_visible();

        let clicked_pane = pane_id_at_position.and_then(|id| {
            Self::gather_clicked_pane_details(
                tab,
                id,
                &event.position,
                active_pane_id,
                event,
                client_id,
            )
        });

        let (pinned_selectable, pinned_unselectable) = if !floating_visible {
            let selectable = tab
                .floating_panes
                .get_pinned_pane_id_at(&event.position, true)
                .ok()
                .flatten();
            let unselectable = tab
                .floating_panes
                .get_pinned_pane_id_at(&event.position, false)
                .ok()
                .flatten();
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
            advanced_mouse_actions: tab.advanced_mouse_actions,
            pinned_selectable,
            pinned_unselectable,
            focus_follows_mouse: tab.focus_follows_mouse,
            mouse_click_through: tab.mouse_click_through,
            mouse_scroll_resize: tab.mouse_scroll_resize,
            passthrough_pane_id,
            context_menu_enabled: tab.context_menu_enabled,
            mousebinds: Arc::new(Mousebinds::default()),
            input_mode: InputMode::default(),
            click_count: 1,
            press_owner: None,
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
        let is_hidden_stack_list_member = tab.pane_is_hidden_stack_list_member(&pane_id);
        let pane = Self::get_pane_at(tab, position, false).ok()??;

        let on_frame = !is_hidden_stack_list_member && pane.position_is_on_frame(position);
        let frame_intercepted = on_frame
            && event.event_type == MouseEventType::Press
            && pane.position_is_on_pin_button(position, client_id);
        let is_unselectable_plugin = matches!(pane.pid(), PaneId::Plugin(_)) && !pane.selectable();
        let edge = if on_frame {
            pane.get_edge_at_position(position)
        } else {
            None
        };
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
            is_unselectable_plugin,
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

        let is_floating = tab.floating_panes.panes_contain(&pane_id);

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
    ) -> Result<bool> {
        let err_context = || "failed to continue pane resize with mouse";

        if let Some(resize_state) = &tab.pane_being_resized_with_mouse {
            let pane_still_exists = if resize_state.is_floating {
                tab.floating_panes.panes_contain(&resize_state.pane_id)
            } else {
                tab.tiled_panes.panes_contain(&resize_state.pane_id)
            };
            if !pane_still_exists {
                log::error!(
                    "Pane {:?} being resized with the mouse no longer exists",
                    resize_state.pane_id
                );
                tab.pane_being_resized_with_mouse = None;
                return Ok(false);
            }
        }

        let (pane_id, edge, is_floating, delta_x, delta_y) =
            if let Some(resize_state) = &tab.pane_being_resized_with_mouse {
                let delta_x = current_position.column() as isize
                    - resize_state.start_position.column() as isize;
                let delta_y = current_position.line() - resize_state.start_position.line();

                if delta_x == 0 && delta_y == 0 {
                    return Ok(false);
                }

                (
                    resize_state.pane_id,
                    resize_state.edge,
                    resize_state.is_floating,
                    delta_x,
                    delta_y,
                )
            } else {
                return Ok(true);
            };

        let strategies = edge_and_delta_to_strategies(edge, delta_x, delta_y);

        let resize_result = if is_floating {
            Self::resize_floating_pane_with_strategies(
                tab,
                pane_id,
                &strategies,
                (delta_x.unsigned_abs(), delta_y.unsigned_abs()),
            )
        } else {
            Self::resize_tiled_pane_with_strategies(
                tab,
                pane_id,
                &strategies,
                (delta_x.abs() as f64, delta_y.abs() as f64),
            )
        };
        if resize_result.is_err() {
            tab.pane_being_resized_with_mouse = None;
        }
        resize_result.with_context(err_context)?;

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
    ) -> Result<bool> {
        let err_context = || "failed to stop pane resize with mouse";

        let start_geom = tab
            .pane_being_resized_with_mouse
            .as_ref()
            .map(|p| p.start_geom.clone());
        let pane_id = tab
            .pane_being_resized_with_mouse
            .as_ref()
            .map(|p| p.pane_id);
        let resize_result = Self::continue_pane_resize_with_mouse(tab, final_position, client_id)
            .with_context(err_context);
        tab.pane_being_resized_with_mouse = None;
        resize_result?;
        let last_geom = pane_id
            .and_then(|pane_id| tab.get_pane_with_id(pane_id))
            .map(|p| p.position_and_size());
        let never_resized = match (start_geom, last_geom) {
            (Some(start_geom), Some(last_geom)) => start_geom == last_geom,
            _ => false,
        };

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
    ) -> Result<()> {
        let err_context = || format!("failed to resize tiled pane {pane_id:?}");

        let viewport = tab.viewport.borrow();
        let viewport_cols = viewport.cols;
        let viewport_rows = viewport.rows;

        let change_by_percent = (
            if viewport_cols > 0 {
                (change_by.0 / viewport_cols as f64) * 100.0
            } else {
                0.0
            },
            if viewport_rows > 0 {
                (change_by.1 / viewport_rows as f64) * 100.0
            } else {
                0.0
            },
        );

        tab.tiled_panes
            .resize_pane_with_strategies(pane_id, strategies, change_by_percent)
            .with_context(err_context)?;

        tab.swap_layouts.set_is_tiled_damaged();

        Ok(())
    }

    fn resize_tiled_pane_with_stacked_resize(
        tab: &mut Tab,
        pane_id: PaneId,
        strategy: &ResizeStrategy,
    ) -> Result<()> {
        let err_context = || format!("failed to resize tiled pane {pane_id:?}");

        tab.tiled_panes
            .stacked_resize_pane_with_id(pane_id, strategy, Some((5.0, 5.0)))
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
        let err_context =
            || format!("failed to execute mouse action {action:?} for client {client_id}");

        let preserves_help_text = matches!(&action, MouseAction::UpdateHover { .. })
            || matches!(
                &action,
                MouseAction::SendToTerminal { event, .. }
                    if event.event_type == MouseEventType::Motion
            );
        if !preserves_help_text {
            tab.mouse_help_text_visible.remove(&client_id);
        }

        match action {
            MouseAction::GroupToggle(pane_id) => {
                if let Some(pane) = tab.get_pane_with_id_mut(pane_id) {
                    let relative_position = pane.relative_position(&event.position);
                    if let Some((hit_plugin_id, pattern, matched_string, context)) =
                        pane.plugin_highlight_at(&relative_position)
                    {
                        let _ = tab
                            .senders
                            .send_to_plugin(PluginInstruction::HighlightClicked {
                                plugin_id: hit_plugin_id,
                                client_id,
                                pane_id,
                                pattern,
                                matched_string,
                                context,
                            });
                        return Ok(MouseEffect::state_changed());
                    }
                }
                Ok(MouseEffect::group_toggle(pane_id))
            },
            MouseAction::GroupAdd(pane_id) => Ok(MouseEffect::group_add(pane_id)),
            MouseAction::Ungroup => Ok(MouseEffect::ungroup()),
            MouseAction::StartResize {
                pane_id,
                edge,
                is_floating: _,
                position,
            } => {
                clear_hover_for_client(tab, client_id);
                Self::start_pane_resize_with_mouse(tab, pane_id, edge, position, client_id)
                    .with_context(err_context)?;
                Ok(MouseEffect::state_changed())
            },
            MouseAction::ContinueResize { position } => {
                let state_changed = Self::continue_pane_resize_with_mouse(tab, position, client_id)
                    .with_context(err_context)?;
                if state_changed {
                    Ok(MouseEffect::state_changed())
                } else {
                    Ok(MouseEffect::default())
                }
            },
            MouseAction::StopResize { position } => {
                Self::execute_stop_resize(tab, position, client_id)
            },
            MouseAction::FocusPane {
                pane_id: _,
                position,
            } => Self::execute_focus_pane(tab, position, event.left, client_id),
            MouseAction::FocusPaneAndClickThrough {
                pane_id: _,
                position,
                event: click_event,
            } => Self::execute_focus_pane_and_click_through(tab, position, click_event, client_id),
            MouseAction::ShowFloatingPanesAndFocus { pane_id } => {
                tab.show_floating_panes();
                tab.floating_panes.focus_pane(pane_id, client_id);
                Ok(MouseEffect::state_changed())
            },
            MouseAction::StartSelection {
                pane_id,
                position,
                modifiers,
            } => {
                let osc133_command_selection = tab.osc133_command_selection;
                let word_separators = tab.word_separators.clone();
                let pane = tab
                    .get_pane_with_id_mut(pane_id)
                    .ok_or_else(|| anyhow!("Failed to find pane {pane_id:?}"))?;
                let relative_position = pane.relative_position(&position);
                pane.set_mouse_modifiers(modifiers);

                let mut leave_clipboard_message = false;
                pane.set_selection_options(osc133_command_selection, &word_separators);
                pane.start_selection(&relative_position, client_id);
                if pane.get_selected_text(client_id).is_some() {
                    leave_clipboard_message = true;
                }
                if pane.supports_mouse_selection() || matches!(pane_id, PaneId::Plugin(_)) {
                    tab.selecting_with_mouse_in_pane = Some(pane_id);
                }
                if leave_clipboard_message {
                    Ok(MouseEffect::state_changed_and_leave_clipboard_message())
                } else {
                    Ok(MouseEffect::default())
                }
            },
            MouseAction::UpdateSelection { position } => {
                if let Some(pane_id_with_selection) = tab.selecting_with_mouse_in_pane {
                    if let Some(pane_with_selection) =
                        tab.get_pane_with_id_mut(pane_id_with_selection)
                    {
                        let relative_position = pane_with_selection.relative_position(&position);
                        pane_with_selection.update_selection(&relative_position, client_id);
                    }
                }
                Ok(MouseEffect::default())
            },
            MouseAction::EndSelection { position } => {
                Self::execute_end_selection(tab, position, client_id)
            },
            MouseAction::StartMovingFloatingPane { position } => {
                Self::execute_start_moving_floating_pane(tab, position)
            },
            MouseAction::ContinueMovingFloatingPane { position } => {
                Self::execute_move_floating_pane(tab, position)
            },
            MouseAction::StopMovingFloatingPane { position } => {
                Self::execute_stop_moving_floating_pane(tab, position, client_id)
            },
            MouseAction::ScrollUp {
                pane_id: _,
                lines,
                app_first,
            } => Self::handle_scrollwheel_up(tab, &event.position, lines, app_first, client_id)
                .with_context(err_context),
            MouseAction::ScrollDown {
                pane_id: _,
                lines,
                app_first,
            } => Self::handle_scrollwheel_down(tab, &event.position, lines, app_first, client_id)
                .with_context(err_context),
            MouseAction::ScrollLeft {
                pane_id: _,
                cols,
                app_first,
            } => Self::handle_scrollwheel_left(tab, &event.position, cols, app_first, client_id)
                .with_context(err_context),
            MouseAction::ScrollRight {
                pane_id: _,
                cols,
                app_first,
            } => Self::handle_scrollwheel_right(tab, &event.position, cols, app_first, client_id)
                .with_context(err_context),
            MouseAction::PassWheelToApp { pane_id, button } => {
                Self::execute_pass_wheel_to_app(tab, pane_id, button, event, client_id)
                    .with_context(err_context)
            },
            MouseAction::ToggleFullscreen { pane_id } => {
                clear_hover_for_client(tab, client_id);
                tab.toggle_pane_fullscreen(pane_id);
                tab.set_force_render();
                Ok(MouseEffect::state_changed())
            },
            MouseAction::RunActions { pane_id, actions } => {
                Self::execute_run_actions(tab, pane_id, actions, event, client_id)
            },
            MouseAction::ResizeScrollUp { pane_id } => {
                Self::handle_resize_scroll_up(tab, pane_id, client_id).with_context(err_context)
            },
            MouseAction::ResizeScrollDown { pane_id } => {
                Self::handle_resize_scroll_down(tab, pane_id, client_id).with_context(err_context)
            },
            MouseAction::ScrollToPreviousPrompt { pane_id, app_first } => {
                Self::handle_prompt_jump(tab, pane_id, true, app_first, event, client_id)
            },
            MouseAction::ScrollToNextPrompt { pane_id, app_first } => {
                Self::handle_prompt_jump(tab, pane_id, false, app_first, event, client_id)
            },
            MouseAction::UpdateHover { pane_id, position } => {
                Self::execute_update_hover(tab, pane_id, position, client_id)
            },
            MouseAction::FocusOnHover { pane_id, position } => {
                Self::execute_focus_on_hover(tab, pane_id, position, client_id)
            },
            MouseAction::SendToTerminal { pane_id, event } => {
                Self::execute_send_to_terminal(tab, pane_id, event, client_id)
            },
            MouseAction::FrameIntercepted { pane_id } => {
                if let Some(pane) = tab.get_pane_with_id_mut(pane_id) {
                    pane.intercept_mouse_event_on_frame(event, client_id);
                }
                tab.set_force_render();
                Ok(MouseEffect::state_changed())
            },
            MouseAction::OpenContextMenu {
                kind,
                pane_id,
                is_floating,
                position,
            } => Ok(MouseEffect::open_context_menu(ContextMenuRequest {
                kind,
                pane_id,
                is_floating,
                position,
            })),
            MouseAction::ForwardRightClickToPlugin { pane_id, position } => {
                if let Some(pane) = tab.get_pane_with_id_mut(pane_id) {
                    let relative_position = pane.relative_position(&position);
                    pane.handle_right_click(&relative_position, client_id);
                }
                Ok(MouseEffect::default())
            },
            MouseAction::NoAction => Ok(MouseEffect::default()),
        }
    }

    fn execute_run_actions(
        tab: &mut Tab,
        pane_id: PaneId,
        actions: Vec<Action>,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let is_selectable = tab
            .get_pane_with_id(pane_id)
            .map(|pane| pane.selectable())
            .unwrap_or(false);
        let is_active = tab.get_active_pane_id(client_id) == Some(pane_id);
        let mut state_changed = false;
        if is_selectable && !is_active {
            clear_hover_for_client(tab, client_id);
            Self::focus_pane_at(tab, &event.position, client_id)?;
            state_changed = true;
        }
        Ok(MouseEffect {
            state_changed,
            run_actions: Some(actions),
            ..Default::default()
        })
    }

    fn execute_pass_wheel_to_app(
        tab: &mut Tab,
        pane_id: PaneId,
        button: MouseButton,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let point = event.position;
        let Some(pane) = tab.get_pane_with_id_mut(pane_id) else {
            return Ok(MouseEffect::default());
        };
        if matches!(pane_id, PaneId::Plugin(_)) {
            match button {
                MouseButton::ScrollUp => pane.scroll_up(3, client_id),
                MouseButton::ScrollDown => pane.scroll_down(3, client_id),
                MouseButton::ScrollLeft => pane.scroll_left(4, client_id),
                MouseButton::ScrollRight => pane.scroll_right(4, client_id),
                _ => {},
            }
            return Ok(MouseEffect::default());
        }
        let relative_position = pane.relative_position(&point);
        let report = match button {
            MouseButton::ScrollUp => pane.mouse_scroll_up(&relative_position),
            MouseButton::ScrollDown => pane.mouse_scroll_down(&relative_position),
            MouseButton::ScrollLeft => pane.mouse_scroll_left(&relative_position),
            MouseButton::ScrollRight => pane.mouse_scroll_right(&relative_position),
            _ => None,
        };
        if let Some(report) = report {
            tab.write_to_terminal_at(report.into_bytes(), &point, client_id)?;
            return Ok(MouseEffect::default());
        }
        let arrow = match button {
            MouseButton::ScrollUp => Some("\u{1b}[A"),
            MouseButton::ScrollDown => Some("\u{1b}[B"),
            _ => None,
        };
        if let Some(arrow) = arrow {
            if pane.is_alternate_mode_active() {
                for _ in 0..3 {
                    tab.write_to_terminal_at(arrow.as_bytes().to_owned(), &point, client_id)?;
                }
            }
        }
        Ok(MouseEffect::default())
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
            let active_pane_id = tab
                .get_active_pane_id(client_id)
                .ok_or_else(|| anyhow!("Failed to find active pane"))?;
            if let Some(pane_id) = pane_id_at_position {
                if pane_id != active_pane_id {
                    Self::focus_pane_at(tab, &position, client_id).with_context(err_context)?;
                }
            }
        }
        Ok(MouseEffect::state_changed())
    }

    fn execute_focus_pane(
        tab: &mut Tab,
        position: Position,
        left_button: bool,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || "failed to focus pane";
        let clicked_unselectable_plugin = Self::unselectable_pane_at_position(tab, &position)
            .map(|pane| pane.pid())
            .filter(|pane_id| matches!(pane_id, PaneId::Plugin(_)));
        clear_hover_for_client_keeping_plugin(tab, client_id, clicked_unselectable_plugin);
        let active_pane_id_before = tab
            .get_active_pane_id(client_id)
            .ok_or_else(|| anyhow!("Failed to find active pane"))?;

        if Self::unselectable_pane_at_position(tab, &position).is_none() {
            Self::focus_pane_at(tab, &position, client_id).with_context(err_context)?;
        }

        let osc133_command_selection = tab.osc133_command_selection;
        let word_separators = tab.word_separators.clone();
        let mut dragging_in_plugin = None;
        if let Some(pane_at_position) = Self::unselectable_pane_at_position(tab, &position) {
            let relative_position = pane_at_position.relative_position(&position);
            pane_at_position.set_selection_options(osc133_command_selection, &word_separators);
            pane_at_position.start_selection(&relative_position, client_id);
            let pane_id = pane_at_position.pid();
            if left_button
                && matches!(pane_id, PaneId::Plugin(_))
                && !pane_at_position.supports_mouse_selection()
            {
                dragging_in_plugin = Some(pane_id);
            }
        }
        if dragging_in_plugin.is_some() {
            tab.selecting_with_mouse_in_pane = dragging_in_plugin;
        }

        if tab.floating_panes.panes_are_visible() {
            let search_selectable = false;
            let moved_pane_with_mouse = tab
                .floating_panes
                .move_pane_with_mouse(position, search_selectable);
            if moved_pane_with_mouse && dragging_in_plugin.is_some() {
                tab.selecting_with_mouse_in_pane = None;
            }
            let active_pane_id_after = tab
                .get_active_pane_id(client_id)
                .ok_or_else(|| anyhow!("Failed to find active pane"))?;
            if moved_pane_with_mouse || active_pane_id_before != active_pane_id_after {
                return Ok(MouseEffect::state_changed());
            } else {
                return Ok(MouseEffect::default());
            }
        }

        let active_pane_id_after = tab
            .get_active_pane_id(client_id)
            .ok_or_else(|| anyhow!("Failed to find active pane"))?;
        if active_pane_id_before != active_pane_id_after {
            Ok(MouseEffect::state_changed())
        } else {
            Ok(MouseEffect::default())
        }
    }

    fn execute_focus_pane_and_click_through(
        tab: &mut Tab,
        position: Position,
        click_event: MouseEvent,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || "failed to focus pane and click through";

        clear_hover_for_client(tab, client_id);
        Self::focus_pane_at(tab, &position, client_id).with_context(err_context)?;

        let osc133_command_selection = tab.osc133_command_selection;
        let word_separators = tab.word_separators.clone();
        if let Some(pane_at_position) = Self::unselectable_pane_at_position(tab, &position) {
            let relative_position = pane_at_position.relative_position(&position);
            pane_at_position.set_selection_options(osc133_command_selection, &word_separators);
            pane_at_position.start_selection(&relative_position, client_id);
            return Ok(MouseEffect::state_changed());
        }

        let active_pane_id = tab
            .get_active_pane_id(client_id)
            .ok_or_else(|| anyhow!("Failed to find active pane"))
            .with_context(err_context)?;

        let pane = tab
            .get_pane_with_id(active_pane_id)
            .ok_or_else(|| anyhow!("Failed to find pane {active_pane_id:?}"))
            .with_context(err_context)?;

        let terminal_wants_mouse = pane.terminal_emulator_wants_mouse();

        if terminal_wants_mouse {
            let relative_position = pane.relative_position(&click_event.position);
            let mut event_for_pane = click_event;
            event_for_pane.position = relative_position;
            if let Some(mouse_event) = pane.mouse_event(&event_for_pane, client_id) {
                if !pane.position_is_on_frame(&click_event.position) {
                    tab.write_to_active_terminal(&None, mouse_event.into_bytes(), false, client_id)
                        .with_context(err_context)?;
                }
            }
        } else {
            if let Some(pane) = tab.get_pane_with_id_mut(active_pane_id) {
                let relative_position = pane.relative_position(&position);
                pane.set_selection_options(osc133_command_selection, &word_separators);
                pane.start_selection(&relative_position, client_id);
                if pane.supports_mouse_selection() || matches!(active_pane_id, PaneId::Plugin(_)) {
                    tab.selecting_with_mouse_in_pane = Some(active_pane_id);
                }
            }
        }

        Ok(MouseEffect::state_changed())
    }

    fn forget_selection_in_missing_pane(tab: &mut Tab) {
        if let Some(pane_id) = tab.selecting_with_mouse_in_pane {
            if tab.get_pane_with_id(pane_id).is_none() {
                tab.selecting_with_mouse_in_pane = None;
            }
        }
    }

    fn execute_end_selection(
        tab: &mut Tab,
        position: Position,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || "failed to end selection";
        Self::forget_selection_in_missing_pane(tab);
        let mut leave_clipboard_message = false;
        let copy_on_release = tab.copy_on_select;

        if let Some(pane_with_selection) = tab
            .selecting_with_mouse_in_pane
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

            if let Some(mouse_event) =
                pane_with_selection.mouse_left_click_release(&relative_position)
            {
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

    fn execute_start_moving_floating_pane(
        tab: &mut Tab,
        position: Position,
    ) -> Result<MouseEffect> {
        let search_selectable = false;
        if tab
            .floating_panes
            .move_pane_with_mouse_from_anywhere(position, search_selectable)
        {
            tab.swap_layouts.set_is_floating_damaged();
            tab.set_force_render();
            Ok(MouseEffect::state_changed())
        } else {
            Ok(MouseEffect::default())
        }
    }

    fn execute_move_floating_pane(tab: &mut Tab, position: Position) -> Result<MouseEffect> {
        let search_selectable = false;
        if tab
            .floating_panes
            .move_pane_with_mouse(position, search_selectable)
        {
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
            let active_pane_id = tab
                .get_active_pane_id(client_id)
                .ok_or_else(|| anyhow!("Failed to find active pane"))?;
            let pane_id_at_position = Self::get_pane_at(tab, &position, false)
                .with_context(err_context)?
                .ok_or_else(|| anyhow!("Failed to find pane at position"))?
                .pid();
            if active_pane_id != pane_id_at_position {
                Self::focus_pane_at(tab, &position, client_id).with_context(err_context)?;
            }
        }
        Ok(MouseEffect::default())
    }

    fn execute_focus_on_hover(
        tab: &mut Tab,
        pane_id: PaneId,
        position: Position,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || format!("failed to focus pane on hover for client {client_id}");

        let is_selectable = tab
            .get_pane_with_id(pane_id)
            .map(|p| p.selectable())
            .unwrap_or(false);
        if !is_selectable {
            return Self::execute_update_hover(tab, Some(pane_id), Some(position), client_id);
        }

        let floating_visible = tab.floating_panes.panes_are_visible();
        let is_floating = tab.floating_panes.get_pane(pane_id).is_some();
        if floating_visible && !is_floating {
            return Self::execute_update_hover(tab, Some(pane_id), Some(position), client_id);
        }

        let is_stacked_one_liner = tab
            .get_pane_with_id(pane_id)
            .map(|p| {
                let geom = p.current_geom();
                geom.is_stacked() && geom.rows.is_fixed()
            })
            .unwrap_or(false);
        if is_stacked_one_liner {
            return Self::execute_update_hover(tab, Some(pane_id), Some(position), client_id);
        }

        if tab.pane_is_hidden_stack_list_member(&pane_id) {
            return Self::execute_update_hover(tab, Some(pane_id), Some(position), client_id);
        }

        let active_pane_id = tab.get_active_pane_id(client_id);
        if active_pane_id == Some(pane_id) {
            return Self::execute_update_hover(tab, Some(pane_id), Some(position), client_id);
        }

        Self::focus_pane_at(tab, &position, client_id).with_context(err_context)?;

        clear_hover_for_client(tab, client_id);

        Ok(MouseEffect::state_changed())
    }

    fn execute_update_hover(
        tab: &mut Tab,
        pane_id: Option<PaneId>,
        position: Option<Position>,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let mut should_render = false;
        let previous_hover_pane_id = tab.mouse_hover_pane_id.get(&client_id).copied();

        if tab.mouse_hover_effects {
            let previous_plugin_hover_pane_id = tab.plugin_hover_pane_id.get(&client_id).copied();
            let current_plugin_hover_pane_id = match pane_id {
                Some(pid) if matches!(pid, PaneId::Plugin(_)) => Some(pid),
                _ => None,
            };
            if let (Some(pid), Some(position)) = (current_plugin_hover_pane_id, position) {
                if let Some(pane) = tab.get_pane_with_id(pid) {
                    let relative_position = pane.relative_position(&position);
                    let _ = pane.mouse_event(
                        &MouseEvent::new_buttonless_motion(relative_position),
                        client_id,
                    );
                }
            }
            if previous_plugin_hover_pane_id != current_plugin_hover_pane_id {
                if let Some(previous_pid) = previous_plugin_hover_pane_id {
                    if let Some(pane) = tab.get_pane_with_id(previous_pid) {
                        let _ = pane.mouse_event(
                            &MouseEvent::new_buttonless_motion(Position::new(0, u16::MAX)),
                            client_id,
                        );
                    }
                }
                match current_plugin_hover_pane_id {
                    Some(pid) => {
                        tab.plugin_hover_pane_id.insert(client_id, pid);
                    },
                    None => {
                        tab.plugin_hover_pane_id.remove(&client_id);
                    },
                }
            }
        }
        match pane_id {
            Some(pid) => {
                if let Some(pane) = tab.get_pane_with_id(pid) {
                    let pane_is_selectable = pane.selectable();
                    if tab.advanced_mouse_actions && tab.mouse_hover_effects && pane_is_selectable {
                        tab.mouse_hover_pane_id.insert(client_id, pid);
                    } else if tab.advanced_mouse_actions || !tab.mouse_hover_effects {
                        tab.mouse_hover_pane_id.remove(&client_id);
                    }
                    tab.mouse_last_pane_id.insert(client_id, pid);
                    should_render = true;
                }
            },
            None => {
                tab.mouse_last_pane_id.remove(&client_id);
                let removed = tab.mouse_hover_pane_id.remove(&client_id);
                if removed.is_some() {
                    should_render = true;
                }
            },
        }

        if let Some(prev_pane_id) = previous_hover_pane_id {
            if Some(prev_pane_id) != pane_id {
                if let Some(pane) = tab.get_pane_with_id_mut(prev_pane_id) {
                    pane.set_hover_position(None);
                }
            }
        }

        if tab.mouse_help_text_visible.remove(&client_id).is_some() {
            should_render = true;
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
        let active_pane_id = tab
            .get_active_pane_id(client_id)
            .ok_or_else(|| anyhow!("Failed to find active pane"))?;
        if pane_id == active_pane_id {
            let pane = tab
                .get_pane_with_id(pane_id)
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
            if clear_hover_for_client(tab, client_id) {
                should_render = true;
            }
            if event.event_type == MouseEventType::Motion {
                if let Some(pane) = tab.get_pane_with_id_mut(pane_id) {
                    if !pane.terminal_emulator_wants_mouse() {
                        let relative = pane.relative_position(&event.position);
                        if pane.set_hover_position(Some(relative)) {
                            should_render = true;
                        }
                    }
                }
            }

            if event.event_type == MouseEventType::Motion && tab.mouse_hover_effects {
                tab.last_mouse_activity_time
                    .insert(client_id, Instant::now());
                let entered_pane = tab.mouse_last_pane_id.get(&client_id) != Some(&pane_id);
                tab.mouse_last_pane_id.insert(client_id, pane_id);
                if entered_pane && tab.mouse_hover_tips {
                    let was_visible = tab
                        .mouse_help_text_visible
                        .get(&client_id)
                        .copied()
                        .unwrap_or(false);
                    tab.mouse_help_text_visible.insert(client_id, true);
                    if !was_visible {
                        should_render = true;
                    }

                    tab.senders
                        .send_to_background_jobs(BackgroundJob::ClearHelpText { client_id })
                        .with_context(err_context)?;
                }
            }
        }
        let mouse_effect = if should_render {
            MouseEffect::state_changed()
        } else {
            MouseEffect::default()
        };
        Ok(mouse_effect)
    }

    fn determine_mouse_action(event: &MouseEvent, ctx: &MouseEventContext) -> Result<MouseAction> {
        if ctx.pane_being_resized {
            return Ok(match event.event_type {
                MouseEventType::Motion => MouseAction::ContinueResize {
                    position: event.position,
                },
                MouseEventType::Release => MouseAction::StopResize {
                    position: event.position,
                },
                _ => MouseAction::NoAction,
            });
        }

        if ctx.selecting_with_mouse {
            return Ok(match event.event_type {
                MouseEventType::Motion if event.left => MouseAction::UpdateSelection {
                    position: event.position,
                },
                MouseEventType::Release if event.left => MouseAction::EndSelection {
                    position: event.position,
                },
                _ => MouseAction::NoAction,
            });
        }

        if ctx.pane_being_moved {
            return Ok(match event.event_type {
                MouseEventType::Motion if event.left => MouseAction::ContinueMovingFloatingPane {
                    position: event.position,
                },
                MouseEventType::Release if event.left => MouseAction::StopMovingFloatingPane {
                    position: event.position,
                },
                _ => MouseAction::NoAction,
            });
        }

        if event.alt {
            if let Some(action) = Self::passthrough_pane_action(event, ctx) {
                return Ok(action);
            }
        }

        let button = mouse_button_of_event(event);
        let is_wheel = button.map(|button| button.is_wheel()).unwrap_or(false);

        if event.event_type != MouseEventType::Press && !is_wheel {
            let has_button = event.left || event.right || event.middle;
            if !has_button {
                if event.event_type == MouseEventType::Motion && !event.alt {
                    return Ok(Self::hover_action(event, ctx));
                }
                return Ok(MouseAction::NoAction);
            }
            return Ok(Self::follow_up_action(event, ctx));
        }

        let Some(button) = button else {
            return Ok(MouseAction::NoAction);
        };

        if button == MouseButton::Left && !event.alt {
            if let Some(details) = ctx.clicked_pane.as_ref() {
                if details.on_frame && details.frame_intercepted {
                    return Ok(MouseAction::FrameIntercepted {
                        pane_id: details.pane_id,
                    });
                }
            }
        }

        let on_frame = ctx
            .clicked_pane
            .as_ref()
            .map(|details| details.on_frame)
            .unwrap_or(false);
        let binding = ctx
            .mousebinds
            .resolve(
                ctx.input_mode,
                button,
                &mouse_modifiers_of_event(event),
                ctx.click_count,
                on_frame,
            )
            .map(|(_, binding)| binding.clone())
            .unwrap_or_else(|| MouseBinding::behaviour(MouseBehaviour::PassToApp));
        let app_first = binding.app_first();

        if !button.is_wheel() && app_first {
            if let Some(details) = ctx.clicked_pane.as_ref() {
                if Some(details.pane_id) == ctx.active_pane_id
                    && !details.on_frame
                    && details.terminal_wants_mouse
                {
                    return Ok(MouseAction::SendToTerminal {
                        pane_id: details.pane_id,
                        event: *event,
                    });
                }
            }
        }

        match binding.kind {
            MouseBindingKind::Actions(actions) => Ok(match ctx.pane_id_at_position {
                Some(pane_id) => MouseAction::RunActions { pane_id, actions },
                None => MouseAction::NoAction,
            }),
            MouseBindingKind::Behaviour(behaviour) => Ok(Self::behaviour_action(
                behaviour, app_first, button, event, ctx,
            )),
        }
    }

    fn passthrough_pane_action(event: &MouseEvent, ctx: &MouseEventContext) -> Option<MouseAction> {
        let passthrough_pane_id = ctx.passthrough_pane_id?;
        let details = ctx.clicked_pane.as_ref()?;
        if details.pane_id == passthrough_pane_id
            && !details.on_frame
            && details.terminal_wants_mouse
        {
            Some(MouseAction::SendToTerminal {
                pane_id: details.pane_id,
                event: *event,
            })
        } else {
            None
        }
    }

    fn follow_up_action(event: &MouseEvent, ctx: &MouseEventContext) -> MouseAction {
        match ctx.press_owner {
            Some(MousePressOwner::App(pane_id)) => MouseAction::SendToTerminal {
                pane_id,
                event: *event,
            },
            Some(MousePressOwner::Group) => {
                match (event.left, event.event_type, ctx.pane_id_at_position) {
                    (true, MouseEventType::Motion, Some(pane_id)) => MouseAction::GroupAdd(pane_id),
                    _ => MouseAction::NoAction,
                }
            },
            Some(MousePressOwner::Swallow) => MouseAction::NoAction,
            None => {
                if event.alt {
                    return match (event.left, event.event_type, ctx.pane_id_at_position) {
                        (true, MouseEventType::Motion, Some(pane_id)) => {
                            MouseAction::GroupAdd(pane_id)
                        },
                        _ => MouseAction::NoAction,
                    };
                }
                let Some(details) = &ctx.clicked_pane else {
                    return MouseAction::NoAction;
                };
                let is_active_pane = Some(details.pane_id) == ctx.active_pane_id;
                if is_active_pane && details.terminal_wants_mouse {
                    MouseAction::SendToTerminal {
                        pane_id: details.pane_id,
                        event: *event,
                    }
                } else {
                    MouseAction::NoAction
                }
            },
        }
    }

    fn hover_action(event: &MouseEvent, ctx: &MouseEventContext) -> MouseAction {
        let Some(pane_id) = ctx.pane_id_at_position else {
            return MouseAction::UpdateHover {
                pane_id: None,
                position: None,
            };
        };
        let is_active_pane = Some(pane_id) == ctx.active_pane_id;
        if is_active_pane {
            return MouseAction::SendToTerminal {
                pane_id,
                event: *event,
            };
        }
        if ctx.focus_follows_mouse {
            return MouseAction::FocusOnHover {
                pane_id,
                position: event.position,
            };
        }
        MouseAction::UpdateHover {
            pane_id: Some(pane_id),
            position: Some(event.position),
        }
    }

    fn behaviour_action(
        behaviour: MouseBehaviour,
        app_first: bool,
        button: MouseButton,
        event: &MouseEvent,
        ctx: &MouseEventContext,
    ) -> MouseAction {
        let details = ctx.clicked_pane.as_ref();
        let is_active = |pane_id: PaneId| Some(pane_id) == ctx.active_pane_id;
        match behaviour {
            MouseBehaviour::Click => {
                if button == MouseButton::Left {
                    Self::left_click_action(event, ctx)
                } else {
                    match details {
                        Some(details) if is_active(details.pane_id) => {
                            Self::pass_to_app_action(button, event, ctx)
                        },
                        _ => Self::focus_action(event, ctx),
                    }
                }
            },
            MouseBehaviour::Select => match details {
                Some(details) if button == MouseButton::Left && !details.on_frame => {
                    if is_active(details.pane_id) && !details.is_unselectable_plugin {
                        MouseAction::StartSelection {
                            pane_id: details.pane_id,
                            position: event.position,
                            modifiers: click_modifiers(event),
                        }
                    } else {
                        Self::left_click_action(event, ctx)
                    }
                },
                _ => MouseAction::NoAction,
            },
            MouseBehaviour::FocusPane => Self::focus_action(event, ctx),
            MouseBehaviour::MovePane => {
                let Some(details) = details else {
                    return MouseAction::NoAction;
                };
                if button != MouseButton::Left {
                    return MouseAction::NoAction;
                }
                let is_pinned_pane = ctx.pinned_selectable == Some(details.pane_id);
                if details.on_frame {
                    if ctx.floating_visible || is_pinned_pane {
                        return MouseAction::StartMovingFloatingPane {
                            position: event.position,
                        };
                    }
                    if let Some(edge) = details.edge {
                        return MouseAction::StartResize {
                            pane_id: details.pane_id,
                            edge,
                            is_floating: false,
                            position: event.position,
                        };
                    }
                    return MouseAction::NoAction;
                }
                if details.is_floating && (ctx.floating_visible || is_pinned_pane) {
                    return MouseAction::StartMovingFloatingPane {
                        position: event.position,
                    };
                }
                MouseAction::NoAction
            },
            MouseBehaviour::ResizePane => match details {
                Some(details) if button == MouseButton::Left && details.on_frame => {
                    match details.edge {
                        Some(edge) => MouseAction::StartResize {
                            pane_id: details.pane_id,
                            edge,
                            is_floating: details.is_floating,
                            position: event.position,
                        },
                        None => MouseAction::NoAction,
                    }
                },
                _ => MouseAction::NoAction,
            },
            MouseBehaviour::ToggleFullscreen => match details {
                Some(details) if !details.is_unselectable_plugin => MouseAction::ToggleFullscreen {
                    pane_id: details.pane_id,
                },
                _ => MouseAction::NoAction,
            },
            MouseBehaviour::GroupToggle => match ctx.pane_id_at_position {
                Some(pane_id) => MouseAction::GroupToggle(pane_id),
                None => MouseAction::NoAction,
            },
            MouseBehaviour::Ungroup => MouseAction::Ungroup,
            MouseBehaviour::ContextMenu => Self::determine_right_button_action(event, ctx),
            MouseBehaviour::Scroll(amount) => match (button, ctx.pane_id_at_position) {
                (MouseButton::ScrollUp, Some(pane_id)) => MouseAction::ScrollUp {
                    pane_id,
                    lines: amount,
                    app_first,
                },
                (MouseButton::ScrollDown, Some(pane_id)) => MouseAction::ScrollDown {
                    pane_id,
                    lines: amount,
                    app_first,
                },
                (MouseButton::ScrollLeft, Some(pane_id)) => MouseAction::ScrollLeft {
                    pane_id,
                    cols: amount,
                    app_first,
                },
                (MouseButton::ScrollRight, Some(pane_id)) => MouseAction::ScrollRight {
                    pane_id,
                    cols: amount,
                    app_first,
                },
                _ => MouseAction::NoAction,
            },
            MouseBehaviour::ScrollColumns(amount) => match (button, ctx.pane_id_at_position) {
                (MouseButton::ScrollUp | MouseButton::ScrollLeft, Some(pane_id)) => {
                    MouseAction::ScrollLeft {
                        pane_id,
                        cols: amount,
                        app_first,
                    }
                },
                (MouseButton::ScrollDown | MouseButton::ScrollRight, Some(pane_id)) => {
                    MouseAction::ScrollRight {
                        pane_id,
                        cols: amount,
                        app_first,
                    }
                },
                _ => MouseAction::NoAction,
            },
            MouseBehaviour::ScrollToPrompt(direction) => {
                if !ctx.advanced_mouse_actions {
                    return MouseAction::NoAction;
                }
                match (direction, ctx.pane_id_at_position) {
                    (PromptDirection::Previous, Some(pane_id)) => {
                        MouseAction::ScrollToPreviousPrompt { pane_id, app_first }
                    },
                    (PromptDirection::Next, Some(pane_id)) => {
                        MouseAction::ScrollToNextPrompt { pane_id, app_first }
                    },
                    _ => MouseAction::NoAction,
                }
            },
            MouseBehaviour::ResizeScroll(direction) => {
                if !ctx.mouse_scroll_resize {
                    return MouseAction::NoAction;
                }
                match (direction, ctx.pane_id_at_position) {
                    (ResizeScrollDirection::Increase, Some(pane_id)) => {
                        MouseAction::ResizeScrollUp { pane_id }
                    },
                    (ResizeScrollDirection::Decrease, Some(pane_id)) => {
                        MouseAction::ResizeScrollDown { pane_id }
                    },
                    _ => MouseAction::NoAction,
                }
            },
            MouseBehaviour::PassToApp => Self::pass_to_app_action(button, event, ctx),
            MouseBehaviour::Ignore => MouseAction::NoAction,
        }
    }

    fn clicked_pinned_unselectable_plugin(ctx: &MouseEventContext) -> bool {
        match (&ctx.clicked_pane, ctx.pinned_unselectable) {
            (Some(details), Some(pinned_id)) => {
                details.is_unselectable_plugin && details.pane_id == pinned_id && !details.on_frame
            },
            _ => false,
        }
    }

    fn focus_action(event: &MouseEvent, ctx: &MouseEventContext) -> MouseAction {
        let Some(details) = &ctx.clicked_pane else {
            return MouseAction::NoAction;
        };
        if !ctx.floating_visible {
            if let Some(pinned_id) = ctx.pinned_selectable {
                return MouseAction::ShowFloatingPanesAndFocus { pane_id: pinned_id };
            }
            if ctx.pinned_unselectable.is_some() && !Self::clicked_pinned_unselectable_plugin(ctx) {
                return MouseAction::NoAction;
            }
        }
        if Some(details.pane_id) == ctx.active_pane_id && !details.is_unselectable_plugin {
            return MouseAction::NoAction;
        }
        MouseAction::FocusPane {
            pane_id: details.pane_id,
            position: event.position,
        }
    }

    fn left_click_action(event: &MouseEvent, ctx: &MouseEventContext) -> MouseAction {
        let Some(details) = &ctx.clicked_pane else {
            return MouseAction::NoAction;
        };

        let is_active_pane = Some(details.pane_id) == ctx.active_pane_id;
        let is_pinned_pane = ctx
            .pinned_selectable
            .map(|id| id == details.pane_id)
            .unwrap_or(false);

        if details.on_frame {
            let should_start_moving = ctx.floating_visible || is_pinned_pane;
            if should_start_moving {
                return MouseAction::StartMovingFloatingPane {
                    position: event.position,
                };
            }

            if let Some(edge) = details.edge {
                return MouseAction::StartResize {
                    pane_id: details.pane_id,
                    edge,
                    is_floating: false,
                    position: event.position,
                };
            }
        }

        if is_active_pane {
            return MouseAction::StartSelection {
                pane_id: details.pane_id,
                position: event.position,
                modifiers: click_modifiers(event),
            };
        }

        if !ctx.floating_visible {
            if let Some(pinned_id) = ctx.pinned_selectable {
                return MouseAction::ShowFloatingPanesAndFocus { pane_id: pinned_id };
            }
            if Self::clicked_pinned_unselectable_plugin(ctx) {
                return MouseAction::FocusPane {
                    pane_id: details.pane_id,
                    position: event.position,
                };
            }
            if ctx.pinned_unselectable.is_some() {
                return MouseAction::NoAction;
            }
        }

        if ctx.mouse_click_through && !ctx.focus_follows_mouse {
            MouseAction::FocusPaneAndClickThrough {
                pane_id: details.pane_id,
                position: event.position,
                event: *event,
            }
        } else {
            MouseAction::FocusPane {
                pane_id: details.pane_id,
                position: event.position,
            }
        }
    }

    fn pass_to_app_action(
        button: MouseButton,
        event: &MouseEvent,
        ctx: &MouseEventContext,
    ) -> MouseAction {
        if button.is_wheel() {
            let Some(pane_id) = ctx.pane_id_at_position else {
                return MouseAction::NoAction;
            };
            let is_plugin = matches!(pane_id, PaneId::Plugin(_));
            if is_plugin || Some(pane_id) == ctx.active_pane_id {
                return MouseAction::PassWheelToApp { pane_id, button };
            }
            return MouseAction::NoAction;
        }
        let Some(details) = &ctx.clicked_pane else {
            return MouseAction::NoAction;
        };
        if details.is_unselectable_plugin {
            return match button {
                MouseButton::Left => MouseAction::FocusPane {
                    pane_id: details.pane_id,
                    position: event.position,
                },
                MouseButton::Right => MouseAction::ForwardRightClickToPlugin {
                    pane_id: details.pane_id,
                    position: event.position,
                },
                _ => MouseAction::NoAction,
            };
        }
        if Some(details.pane_id) != ctx.active_pane_id {
            return MouseAction::NoAction;
        }
        if matches!(details.pane_id, PaneId::Plugin(_)) {
            return match button {
                MouseButton::Left if !details.on_frame => MouseAction::StartSelection {
                    pane_id: details.pane_id,
                    position: event.position,
                    modifiers: click_modifiers(event),
                },
                MouseButton::Right if !details.on_frame => MouseAction::ForwardRightClickToPlugin {
                    pane_id: details.pane_id,
                    position: event.position,
                },
                _ => MouseAction::NoAction,
            };
        }
        MouseAction::SendToTerminal {
            pane_id: details.pane_id,
            event: *event,
        }
    }

    fn determine_right_button_action(event: &MouseEvent, ctx: &MouseEventContext) -> MouseAction {
        if !ctx.context_menu_enabled {
            return match ctx.pane_id_at_position {
                Some(pane_id) if Some(pane_id) == ctx.active_pane_id => {
                    MouseAction::SendToTerminal {
                        pane_id,
                        event: *event,
                    }
                },
                _ => MouseAction::NoAction,
            };
        }
        let Some(details) = ctx.clicked_pane.as_ref() else {
            return MouseAction::NoAction;
        };
        if event.event_type != MouseEventType::Press {
            return MouseAction::NoAction;
        }
        if details.is_unselectable_plugin {
            return MouseAction::ForwardRightClickToPlugin {
                pane_id: details.pane_id,
                position: event.position,
            };
        }
        let kind = if details.on_frame {
            ContextMenuKind::PaneFrame
        } else {
            ContextMenuKind::Pane
        };
        MouseAction::OpenContextMenu {
            kind,
            pane_id: details.pane_id,
            is_floating: details.is_floating,
            position: event.position,
        }
    }

    fn unselectable_pane_at_position<'a>(
        tab: &'a mut Tab,
        point: &Position,
    ) -> Option<&'a mut Box<dyn Pane>> {
        Self::get_pane_at(tab, point, false)
            .ok()
            .flatten()
            .filter(|pane| !pane.selectable())
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
        if tab.floating_panes.has_pinned_panes() {
            let search_selectable = false;
            if let Some(pane_id) = tab
                .floating_panes
                .get_pinned_pane_id_at(point, search_selectable)
                .with_context(err_context)?
            {
                tab.floating_panes.focus_pane(pane_id, client_id);
                tab.set_pane_active_at(pane_id);
                tab.show_floating_panes();
                return Ok(());
            }
        }
        if let Some(clicked_pane) = tab.get_pane_id_at(point, true).with_context(err_context)? {
            if !tab.focus_hidden_stack_list_member(clicked_pane, client_id) {
                tab.tiled_panes.focus_pane(clicked_pane, client_id);
            }
            tab.set_pane_active_at(clicked_pane);
            if tab.floating_panes.panes_are_visible() {
                tab.hide_floating_panes();
                tab.set_force_render();
            }
        }
        Ok(())
    }

    pub(crate) fn handle_scrollwheel_up(
        tab: &mut Tab,
        point: &Position,
        lines: usize,
        app_first: bool,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || {
            format!("failed to handle scrollwheel up at position {point:?} for client {client_id}")
        };

        if let Some(pane) = Self::get_pane_at(tab, point, false).with_context(err_context)? {
            let relative_position = pane.relative_position(point);
            let app_report = if app_first {
                pane.mouse_scroll_up(&relative_position)
            } else {
                None
            };
            if let Some(mouse_event) = app_report {
                tab.write_to_terminal_at(mouse_event.into_bytes(), point, client_id)
                    .with_context(err_context)?;
            } else if app_first && pane.is_alternate_mode_active() {
                // separate writes so each sequence gets adjusted for cursor keys mode
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

    pub(crate) fn handle_scrollwheel_down(
        tab: &mut Tab,
        point: &Position,
        lines: usize,
        app_first: bool,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || {
            format!(
                "failed to handle scrollwheel down at position {point:?} for client {client_id}"
            )
        };

        if let Some(pane) = Self::get_pane_at(tab, point, false).with_context(err_context)? {
            let relative_position = pane.relative_position(point);
            let app_report = if app_first {
                pane.mouse_scroll_down(&relative_position)
            } else {
                None
            };
            if let Some(mouse_event) = app_report {
                tab.write_to_terminal_at(mouse_event.into_bytes(), point, client_id)
                    .with_context(err_context)?;
            } else if app_first && pane.is_alternate_mode_active() {
                // separate writes so each sequence gets adjusted for cursor keys mode
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

    fn handle_prompt_jump(
        tab: &mut Tab,
        pane_id: PaneId,
        to_previous_prompt: bool,
        app_first: bool,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context =
            || format!("failed to jump to prompt in pane {pane_id:?} for client {client_id}");

        let report_for_pane = tab
            .get_pane_with_id(pane_id)
            .filter(|_| app_first)
            .and_then(|pane| {
                let mut event_for_pane = *event;
                event_for_pane.position = pane.relative_position(&event.position);
                pane.mouse_event(&event_for_pane, client_id)
            });
        if let Some(report_for_pane) = report_for_pane {
            tab.write_to_terminal_at(report_for_pane.into_bytes(), &event.position, client_id)
                .with_context(err_context)?;
            return Ok(MouseEffect::default());
        }

        if let Some(pane) = tab.get_pane_with_id_mut(pane_id) {
            if to_previous_prompt {
                pane.scroll_to_previous_prompt(client_id);
            } else {
                pane.scroll_to_next_prompt(client_id);
            }
        }
        Ok(MouseEffect::state_changed())
    }

    pub(crate) fn handle_scrollwheel_left(
        tab: &mut Tab,
        point: &Position,
        cols: usize,
        app_first: bool,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || {
            format!(
                "failed to handle scrollwheel left at position {point:?} for client {client_id}"
            )
        };

        if let Some(pane) = Self::get_pane_at(tab, point, false).with_context(err_context)? {
            let relative_position = pane.relative_position(point);
            let app_report = if app_first {
                pane.mouse_scroll_left(&relative_position)
            } else {
                None
            };
            if let Some(mouse_event) = app_report {
                tab.write_to_terminal_at(mouse_event.into_bytes(), point, client_id)
                    .with_context(err_context)?;
            } else {
                pane.scroll_left(cols, client_id);
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

    pub(crate) fn handle_scrollwheel_right(
        tab: &mut Tab,
        point: &Position,
        cols: usize,
        app_first: bool,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || {
            format!(
                "failed to handle scrollwheel right at position {point:?} for client {client_id}"
            )
        };

        if let Some(pane) = Self::get_pane_at(tab, point, false).with_context(err_context)? {
            let relative_position = pane.relative_position(point);
            let app_report = if app_first {
                pane.mouse_scroll_right(&relative_position)
            } else {
                None
            };
            if let Some(mouse_event) = app_report {
                tab.write_to_terminal_at(mouse_event.into_bytes(), point, client_id)
                    .with_context(err_context)?;
            } else {
                pane.scroll_right(cols, client_id);
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
    fn handle_resize_scroll_up(
        tab: &mut Tab,
        pane_id: PaneId,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || format!("failed to handle resize scroll up for pane {pane_id:?}");

        let is_floating = tab.floating_panes.panes_contain(&pane_id);

        let strategy = ResizeStrategy {
            resize: Resize::Increase,
            direction: None,
            invert_on_boundaries: false,
        };

        if is_floating {
            Self::resize_floating_pane_with_strategies(tab, pane_id, &[strategy], (5, 2))
                .with_context(err_context)?;
            tab.swap_layouts.set_is_floating_damaged();
        } else {
            let active_pane_id = tab
                .get_active_pane_id(client_id)
                .ok_or_else(|| anyhow!("Failed to find active pane"))?;

            tab.dissolve_stack_lists_for_classic_mutation();
            Self::resize_tiled_pane_with_stacked_resize(tab, active_pane_id, &strategy)
                .with_context(err_context)?;
            tab.tiled_panes.reapply_pane_frames();
            tab.swap_layouts.set_is_tiled_damaged();
        }

        tab.set_force_render();
        Ok(MouseEffect::state_changed())
    }

    fn handle_resize_scroll_down(
        tab: &mut Tab,
        pane_id: PaneId,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || format!("failed to handle resize scroll down for pane {pane_id:?}");

        let is_floating = tab.floating_panes.panes_contain(&pane_id);

        let strategy = ResizeStrategy {
            resize: Resize::Decrease,
            direction: None,
            invert_on_boundaries: false,
        };

        if is_floating {
            Self::resize_floating_pane_with_strategies(tab, pane_id, &[strategy], (5, 2))
                .with_context(err_context)?;
            tab.swap_layouts.set_is_floating_damaged();
        } else {
            let active_pane_id = tab
                .get_active_pane_id(client_id)
                .ok_or_else(|| anyhow!("Failed to find active pane"))?;

            tab.dissolve_stack_lists_for_classic_mutation();
            Self::resize_tiled_pane_with_stacked_resize(tab, active_pane_id, &strategy)
                .with_context(err_context)?;
            tab.tiled_panes.reapply_pane_frames();
            tab.swap_layouts.set_is_tiled_damaged();
        }

        tab.set_force_render();
        Ok(MouseEffect::state_changed())
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
            Ok(tab.get_pane_with_id_mut(pane_id))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn set_mouse_selection_support(
        tab: &mut Tab,
        pane_id: PaneId,
        selection_support: bool,
    ) {
        if let Some(pane) = tab.get_pane_with_id_mut(pane_id) {
            pane.set_mouse_selection_support(selection_support);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mouse_event_context(mouse_scroll_resize: bool) -> MouseEventContext {
        MouseEventContext {
            pane_id_at_position: Some(PaneId::Terminal(1)),
            active_pane_id: Some(PaneId::Terminal(1)),
            floating_visible: false,
            pane_being_resized: false,
            selecting_with_mouse: false,
            pane_being_moved: false,
            clicked_pane: None,
            advanced_mouse_actions: true,
            pinned_selectable: None,
            pinned_unselectable: None,
            focus_follows_mouse: false,
            mouse_click_through: false,
            mouse_scroll_resize,
            passthrough_pane_id: None,
            context_menu_enabled: true,
            mousebinds: zellij_utils::input::config_settings::default_config()
                .mousebinds
                .clone(),
            input_mode: InputMode::Normal,
            click_count: 1,
            press_owner: None,
        }
    }

    fn custom_mousebinds(text: &str) -> Arc<Mousebinds> {
        zellij_utils::input::config::Config::from_kdl(
            text,
            Some(zellij_utils::input::config::Config::from_default_assets().unwrap()),
        )
        .unwrap()
        .mousebinds
        .clone()
    }

    fn record(
        button: MouseButton,
        modifiers: &[KeyModifier],
        position: Position,
        time: Instant,
        count: u8,
    ) -> MouseClickRecord {
        MouseClickRecord {
            button,
            modifiers: modifiers.iter().copied().collect(),
            position,
            time,
            count,
        }
    }

    #[test]
    fn a_quick_second_click_on_the_same_cell_counts_up_to_three_then_starts_again() {
        let now = Instant::now();
        let position = Position::new(2, 3);
        let none = BTreeSet::new();
        assert_eq!(
            next_click_count(None, MouseButton::Left, &none, position, now),
            1
        );
        for (previous_count, expected) in [(1, 2), (2, 3), (3, 1)] {
            let previous = record(MouseButton::Left, &[], position, now, previous_count);
            assert_eq!(
                next_click_count(Some(&previous), MouseButton::Left, &none, position, now),
                expected
            );
        }
    }

    #[test]
    fn a_different_button_modifier_cell_or_a_late_click_starts_counting_again() {
        let now = Instant::now();
        let position = Position::new(2, 3);
        let none = BTreeSet::new();
        let previous = record(MouseButton::Left, &[], position, now, 1);
        assert_eq!(
            next_click_count(Some(&previous), MouseButton::Right, &none, position, now),
            1
        );
        let ctrl: BTreeSet<KeyModifier> = [KeyModifier::Ctrl].into_iter().collect();
        assert_eq!(
            next_click_count(Some(&previous), MouseButton::Left, &ctrl, position, now),
            1
        );
        assert_eq!(
            next_click_count(
                Some(&previous),
                MouseButton::Left,
                &none,
                Position::new(2, 4),
                now
            ),
            1
        );
        let late = now + std::time::Duration::from_millis(CLICK_TIME_THRESHOLD_MS as u64 + 1);
        assert_eq!(
            next_click_count(Some(&previous), MouseButton::Left, &none, position, late),
            1
        );
        let in_time = now + std::time::Duration::from_millis(CLICK_TIME_THRESHOLD_MS as u64);
        assert_eq!(
            next_click_count(Some(&previous), MouseButton::Left, &none, position, in_time),
            2
        );
    }

    #[test]
    fn scroll_columns_on_the_vertical_wheel_scrolls_sideways() {
        let mut context = context_with(clicked(PaneId::Terminal(1)));
        context.mousebinds = custom_mousebinds(
            "mousebinds {\n    shared {\n        bind \"ScrollUp\" { ScrollColumns 5; }\n        bind \"ScrollDown\" { ScrollColumns 6; }\n    }\n}\n",
        );
        let position = Position::new(3, 4);
        assert_eq!(
            MouseHandler::determine_mouse_action(
                &MouseEvent::new_scroll_up_event(position),
                &context
            )
            .unwrap(),
            MouseAction::ScrollLeft {
                pane_id: PaneId::Terminal(1),
                cols: 5,
                app_first: true,
            }
        );
        assert_eq!(
            MouseHandler::determine_mouse_action(
                &MouseEvent::new_scroll_down_event(position),
                &context
            )
            .unwrap(),
            MouseAction::ScrollRight {
                pane_id: PaneId::Terminal(1),
                cols: 6,
                app_first: true,
            }
        );
    }

    #[test]
    fn app_first_false_on_the_wheel_is_carried_to_the_scroll_action() {
        let mut details = clicked(PaneId::Terminal(1));
        details.terminal_wants_mouse = true;
        let mut context = context_with(details);
        context.mousebinds = custom_mousebinds(
            "mousebinds {\n    shared {\n        bind \"ScrollUp\" app_first=false { Scroll 2; }\n    }\n}\n",
        );
        assert_eq!(
            MouseHandler::determine_mouse_action(
                &MouseEvent::new_scroll_up_event(Position::new(3, 4)),
                &context
            )
            .unwrap(),
            MouseAction::ScrollUp {
                pane_id: PaneId::Terminal(1),
                lines: 2,
                app_first: false,
            }
        );
    }

    #[test]
    fn move_pane_dragged_from_inside_a_visible_floating_pane_starts_moving_it() {
        let mut details = clicked(PaneId::Terminal(2));
        details.is_floating = true;
        let mut context = context_with(details);
        context.floating_visible = true;
        context.mousebinds = custom_mousebinds(
            "mousebinds {\n    shared {\n        bind \"Alt Left\" { MovePane; }\n    }\n}\n",
        );
        let position = Position::new(3, 4);
        assert_eq!(
            MouseHandler::determine_mouse_action(
                &MouseEvent::new_left_press_with_alt_event(position),
                &context
            )
            .unwrap(),
            MouseAction::StartMovingFloatingPane { position }
        );
    }

    #[test]
    fn move_pane_inside_a_tiled_pane_does_nothing() {
        let mut context = context_with(clicked(PaneId::Terminal(1)));
        context.mousebinds = custom_mousebinds(
            "mousebinds {\n    shared {\n        bind \"Alt Left\" { MovePane; }\n    }\n}\n",
        );
        assert_eq!(
            MouseHandler::determine_mouse_action(
                &MouseEvent::new_left_press_with_alt_event(Position::new(3, 4)),
                &context
            )
            .unwrap(),
            MouseAction::NoAction
        );
    }

    #[test]
    fn disabled_ctrl_scroll_does_not_fall_through_to_regular_scrolling() {
        let context = mouse_event_context(false);
        let position = Position::new(1, 1);
        let events = [
            MouseEvent::new_ctrl_scroll_up_event(position),
            MouseEvent::new_ctrl_scroll_down_event(position),
        ];

        for event in events {
            assert_eq!(
                MouseHandler::determine_mouse_action(&event, &context).unwrap(),
                MouseAction::NoAction
            );
        }
    }

    fn clicked(pane_id: PaneId) -> ClickedPaneDetails {
        ClickedPaneDetails {
            pane_id,
            on_frame: false,
            frame_intercepted: false,
            edge: None,
            is_floating: false,
            terminal_wants_mouse: false,
            is_unselectable_plugin: false,
        }
    }

    fn context_with(details: ClickedPaneDetails) -> MouseEventContext {
        let mut context = mouse_event_context(true);
        context.pane_id_at_position = Some(details.pane_id);
        context.clicked_pane = Some(details);
        context
    }

    fn open_menu(kind: ContextMenuKind, pane_id: PaneId, position: Position) -> MouseAction {
        MouseAction::OpenContextMenu {
            kind,
            pane_id,
            is_floating: false,
            position,
        }
    }

    #[test]
    fn right_press_on_focused_pane_without_mouse_reporting_opens_menu() {
        let position = Position::new(3, 4);
        let context = context_with(clicked(PaneId::Terminal(1)));
        let event = MouseEvent::new_right_press_event(position);
        assert_eq!(
            MouseHandler::determine_mouse_action(&event, &context).unwrap(),
            open_menu(ContextMenuKind::Pane, PaneId::Terminal(1), position)
        );
    }

    #[test]
    fn right_button_events_go_to_a_focused_program_that_wants_the_mouse() {
        let position = Position::new(3, 4);
        let mut details = clicked(PaneId::Terminal(1));
        details.terminal_wants_mouse = true;
        let context = context_with(details);
        for event in [
            MouseEvent::new_right_press_event(position),
            MouseEvent::new_right_motion_event(position),
            MouseEvent::new_right_release_event(position),
        ] {
            assert_eq!(
                MouseHandler::determine_mouse_action(&event, &context).unwrap(),
                MouseAction::SendToTerminal {
                    pane_id: PaneId::Terminal(1),
                    event,
                }
            );
        }
    }

    #[test]
    fn right_motion_and_release_without_mouse_reporting_do_nothing() {
        let position = Position::new(3, 4);
        let context = context_with(clicked(PaneId::Terminal(1)));
        for event in [
            MouseEvent::new_right_motion_event(position),
            MouseEvent::new_right_release_event(position),
        ] {
            assert_eq!(
                MouseHandler::determine_mouse_action(&event, &context).unwrap(),
                MouseAction::NoAction
            );
        }
    }

    #[test]
    fn right_press_on_unfocused_pane_opens_menu_without_focusing_it() {
        let position = Position::new(3, 4);
        let context = context_with(clicked(PaneId::Terminal(2)));
        let event = MouseEvent::new_right_press_event(position);
        assert_eq!(
            MouseHandler::determine_mouse_action(&event, &context).unwrap(),
            open_menu(ContextMenuKind::Pane, PaneId::Terminal(2), position)
        );
    }

    #[test]
    fn right_press_on_frame_of_mouse_program_opens_frame_menu() {
        let position = Position::new(0, 4);
        let mut details = clicked(PaneId::Terminal(1));
        details.terminal_wants_mouse = true;
        details.on_frame = true;
        let context = context_with(details);
        let event = MouseEvent::new_right_press_event(position);
        assert_eq!(
            MouseHandler::determine_mouse_action(&event, &context).unwrap(),
            open_menu(ContextMenuKind::PaneFrame, PaneId::Terminal(1), position)
        );
    }

    #[test]
    fn right_press_on_pin_button_opens_frame_menu_instead_of_toggling_pin() {
        let position = Position::new(0, 4);
        let mut details = clicked(PaneId::Terminal(1));
        details.on_frame = true;
        details.frame_intercepted = true;
        details.is_floating = true;
        let context = context_with(details);
        let event = MouseEvent::new_right_press_event(position);
        assert_eq!(
            MouseHandler::determine_mouse_action(&event, &context).unwrap(),
            MouseAction::OpenContextMenu {
                kind: ContextMenuKind::PaneFrame,
                pane_id: PaneId::Terminal(1),
                is_floating: true,
                position,
            }
        );
    }

    #[test]
    fn right_press_on_bar_is_forwarded_to_the_bar_plugin() {
        let position = Position::new(0, 10);
        let mut details = clicked(PaneId::Plugin(7));
        details.is_unselectable_plugin = true;
        let context = context_with(details);
        let event = MouseEvent::new_right_press_event(position);
        assert_eq!(
            MouseHandler::determine_mouse_action(&event, &context).unwrap(),
            MouseAction::ForwardRightClickToPlugin {
                pane_id: PaneId::Plugin(7),
                position,
            }
        );
    }

    #[test]
    fn right_press_outside_any_pane_does_nothing() {
        let mut context = mouse_event_context(true);
        context.pane_id_at_position = None;
        let event = MouseEvent::new_right_press_event(Position::new(3, 4));
        assert_eq!(
            MouseHandler::determine_mouse_action(&event, &context).unwrap(),
            MouseAction::NoAction
        );
    }

    #[test]
    fn alt_right_press_still_ungroups() {
        let position = Position::new(3, 4);
        let context = context_with(clicked(PaneId::Terminal(1)));
        assert_eq!(
            MouseHandler::determine_mouse_action(
                &MouseEvent::new_right_press_with_alt_event(position),
                &context
            )
            .unwrap(),
            MouseAction::Ungroup
        );
    }

    #[test]
    fn plain_middle_press_on_focused_pane_is_still_sent_to_the_pane() {
        let position = Position::new(3, 4);
        let context = context_with(clicked(PaneId::Terminal(1)));
        let event = MouseEvent::new_middle_press_event(position);
        assert_eq!(
            MouseHandler::determine_mouse_action(&event, &context).unwrap(),
            MouseAction::SendToTerminal {
                pane_id: PaneId::Terminal(1),
                event,
            }
        );
    }

    fn menu_off(details: ClickedPaneDetails) -> MouseEventContext {
        let mut context = context_with(details);
        context.context_menu_enabled = false;
        context
    }

    #[test]
    fn with_the_menu_off_a_right_press_on_the_focused_pane_goes_to_the_pane() {
        let position = Position::new(3, 4);
        let context = menu_off(clicked(PaneId::Terminal(1)));
        let event = MouseEvent::new_right_press_event(position);
        assert_eq!(
            MouseHandler::determine_mouse_action(&event, &context).unwrap(),
            MouseAction::SendToTerminal {
                pane_id: PaneId::Terminal(1),
                event,
            }
        );
    }

    #[test]
    fn with_the_menu_off_a_right_press_on_the_focused_pane_frame_goes_to_the_pane() {
        let position = Position::new(0, 4);
        let mut details = clicked(PaneId::Terminal(1));
        details.on_frame = true;
        let context = menu_off(details);
        let event = MouseEvent::new_right_press_event(position);
        assert_eq!(
            MouseHandler::determine_mouse_action(&event, &context).unwrap(),
            MouseAction::SendToTerminal {
                pane_id: PaneId::Terminal(1),
                event,
            }
        );
    }

    #[test]
    fn with_the_menu_off_a_right_press_on_an_unfocused_pane_is_dropped() {
        let position = Position::new(3, 4);
        let context = menu_off(clicked(PaneId::Terminal(2)));
        let event = MouseEvent::new_right_press_event(position);
        assert_eq!(
            MouseHandler::determine_mouse_action(&event, &context).unwrap(),
            MouseAction::NoAction
        );
    }

    #[test]
    fn with_the_menu_off_a_right_press_on_a_bar_is_not_forwarded_to_the_bar() {
        let position = Position::new(0, 4);
        let mut details = clicked(PaneId::Plugin(7));
        details.is_unselectable_plugin = true;
        let context = menu_off(details);
        let event = MouseEvent::new_right_press_event(position);
        assert_eq!(
            MouseHandler::determine_mouse_action(&event, &context).unwrap(),
            MouseAction::NoAction
        );
    }

    #[test]
    fn with_the_menu_off_alt_right_press_still_ungroups_and_alt_middle_does_nothing() {
        let position = Position::new(3, 4);
        let context = menu_off(clicked(PaneId::Terminal(1)));
        assert_eq!(
            MouseHandler::determine_mouse_action(
                &MouseEvent::new_right_press_with_alt_event(position),
                &context
            )
            .unwrap(),
            MouseAction::Ungroup
        );
        let mut alt_middle = MouseEvent::new_middle_press_event(position);
        alt_middle.alt = true;
        assert_eq!(
            MouseHandler::determine_mouse_action(&alt_middle, &context).unwrap(),
            MouseAction::NoAction
        );
    }

    #[test]
    fn with_the_menu_on_alt_right_press_never_opens_the_menu() {
        let position = Position::new(3, 4);
        let context = context_with(clicked(PaneId::Terminal(2)));
        assert_eq!(
            MouseHandler::determine_mouse_action(
                &MouseEvent::new_right_press_with_alt_event(position),
                &context
            )
            .unwrap(),
            MouseAction::Ungroup
        );
    }

    #[test]
    fn left_press_on_pin_button_still_toggles_pin() {
        let position = Position::new(0, 4);
        let mut details = clicked(PaneId::Terminal(1));
        details.on_frame = true;
        details.frame_intercepted = true;
        let context = context_with(details);
        let event = MouseEvent::new_left_press_event(position);
        assert_eq!(
            MouseHandler::determine_mouse_action(&event, &context).unwrap(),
            MouseAction::FrameIntercepted {
                pane_id: PaneId::Terminal(1),
            }
        );
    }

    #[test]
    fn ctrl_click_goes_to_a_focused_program_that_wants_the_mouse() {
        let position = Position::new(3, 4);
        let mut details = clicked(PaneId::Terminal(1));
        details.terminal_wants_mouse = true;
        let context = context_with(details);
        let event = MouseEvent::new_left_press_with_ctrl_event(position);
        assert_eq!(
            MouseHandler::determine_mouse_action(&event, &context).unwrap(),
            MouseAction::SendToTerminal {
                pane_id: PaneId::Terminal(1),
                event,
            }
        );
    }

    #[test]
    fn modified_clicks_on_a_focused_plugin_carry_their_modifiers() {
        let position = Position::new(3, 4);
        let mut context = context_with(clicked(PaneId::Plugin(1)));
        context.active_pane_id = Some(PaneId::Plugin(1));
        let ctrl_click = MouseEvent::new_left_press_with_ctrl_event(position);
        let mut shift_click = MouseEvent::new_left_press_event(position);
        shift_click.shift = true;
        for (event, modifier) in [
            (ctrl_click, zellij_utils::data::KeyModifier::Ctrl),
            (shift_click, zellij_utils::data::KeyModifier::Shift),
        ] {
            assert_eq!(
                MouseHandler::determine_mouse_action(&event, &context).unwrap(),
                MouseAction::StartSelection {
                    pane_id: PaneId::Plugin(1),
                    position,
                    modifiers: std::iter::once(modifier).collect(),
                }
            );
        }
    }
}

fn click_modifiers(
    event: &MouseEvent,
) -> std::collections::BTreeSet<zellij_utils::data::KeyModifier> {
    use zellij_utils::data::KeyModifier;
    let mut modifiers = std::collections::BTreeSet::new();
    if event.shift {
        modifiers.insert(KeyModifier::Shift);
    }
    if event.ctrl {
        modifiers.insert(KeyModifier::Ctrl);
    }
    modifiers
}

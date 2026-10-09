use super::{AdjustedInput, Pane, Tab};
use crate::output::Output;
use crate::panes::{PaneId, PluginPane};
use crate::plugins::PluginInstruction;
use crate::ClientId;
use zellij_utils::data::{Event, KeyWithModifier, Mouse, PopupCorner};
use zellij_utils::errors::prelude::*;
use zellij_utils::input::layout::Run;
use zellij_utils::input::mouse::{MouseEvent, MouseEventType};
use zellij_utils::pane_size::{Dimension, Offset, PaneGeom, Viewport};
use zellij_utils::position::Position;

pub const POPUP_Z_INDEX: usize = usize::MAX / 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupPlacement {
    At(Position),
    CenteredIn(Viewport),
    Corner(PopupCorner),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupKind {
    Menu,
    Prompt,
    Modal,
    Info,
}

impl PopupKind {
    pub fn takes_focus(&self) -> bool {
        !matches!(self, PopupKind::Info)
    }
}

pub(crate) struct Popup {
    pub pane: Box<dyn Pane>,
    pub placement: PopupPlacement,
    pub kind: PopupKind,
    pub wanted_cols: usize,
    pub wanted_rows: usize,
    pub anchor_pane: Option<PaneId>,
    pub held_back: bool,
}

impl Popup {
    fn plugin_id(&self) -> Option<u32> {
        match self.pane.pid() {
            PaneId::Plugin(plugin_id) => Some(plugin_id),
            PaneId::Terminal(_) => None,
        }
    }
    fn takes_focus(&self) -> bool {
        self.kind.takes_focus()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupMouseOutcome {
    Consumed,
    CloseRequested,
}

pub fn place_popup(
    anchor: Position,
    wanted_cols: usize,
    wanted_rows: usize,
    bounds: Viewport,
) -> PaneGeom {
    let cols = wanted_cols.max(1).min(bounds.cols.max(1));
    let rows = wanted_rows.max(1).min(bounds.rows.max(1));
    let right_edge = bounds.x + bounds.cols;
    let bottom_edge = bounds.y + bounds.rows;
    let anchor_line = (anchor.line.0.max(0) as usize)
        .max(bounds.y)
        .min(bottom_edge.saturating_sub(1));
    let anchor_column = anchor
        .column
        .0
        .max(bounds.x)
        .min(right_edge.saturating_sub(1));
    let x = if anchor_column + cols <= right_edge {
        anchor_column
    } else {
        right_edge.saturating_sub(cols)
    };
    let y = if anchor_line + rows <= bottom_edge {
        anchor_line
    } else {
        bottom_edge.saturating_sub(rows)
    };
    PaneGeom {
        x,
        y,
        cols: Dimension::fixed(cols),
        rows: Dimension::fixed(rows),
        ..Default::default()
    }
}

pub fn place_popup_with(
    placement: PopupPlacement,
    wanted_cols: usize,
    wanted_rows: usize,
    bounds: Viewport,
) -> PaneGeom {
    match placement {
        PopupPlacement::At(anchor) => place_popup(anchor, wanted_cols, wanted_rows, bounds),
        PopupPlacement::CenteredIn(area) => {
            let cols = wanted_cols.max(1).min(bounds.cols.max(1));
            let rows = wanted_rows.max(1).min(bounds.rows.max(1));
            let x = area.x + area.cols.saturating_sub(cols) / 2;
            let y = area.y + area.rows.saturating_sub(rows) / 2;
            place_popup(Position::new(y as i32, x as u16), cols, rows, bounds)
        },
        PopupPlacement::Corner(corner) => {
            place_popup_in_corner(corner, 0, wanted_cols, wanted_rows, bounds)
        },
    }
}

pub fn place_popup_in_corner(
    corner: PopupCorner,
    rows_taken: usize,
    wanted_cols: usize,
    wanted_rows: usize,
    bounds: Viewport,
) -> PaneGeom {
    let cols = wanted_cols.max(1).min(bounds.cols.max(1));
    let rows = wanted_rows.max(1).min(bounds.rows.max(1));
    let x = if corner.is_left() {
        bounds.x
    } else {
        bounds.x + bounds.cols.saturating_sub(cols)
    };
    let y = if corner.is_top() {
        bounds.y + rows_taken
    } else {
        (bounds.y + bounds.rows).saturating_sub(rows_taken + rows)
    };
    place_popup(Position::new(y as i32, x as u16), cols, rows, bounds)
}

pub fn place_popup_stack(
    popups: &[(PopupPlacement, usize, usize)],
    bounds: Viewport,
) -> Vec<PaneGeom> {
    let mut geoms: Vec<Option<PaneGeom>> = vec![None; popups.len()];
    let mut rows_taken_per_corner: Vec<(PopupCorner, usize)> = vec![];
    for (index, (placement, wanted_cols, wanted_rows)) in popups.iter().enumerate().rev() {
        if let PopupPlacement::Corner(corner) = placement {
            let rows_taken = rows_taken_per_corner
                .iter()
                .find(|(taken_corner, _)| taken_corner == corner)
                .map(|(_, rows_taken)| *rows_taken)
                .unwrap_or(0);
            let geom =
                place_popup_in_corner(*corner, rows_taken, *wanted_cols, *wanted_rows, bounds);
            let rows_taken = rows_taken + geom.rows.as_usize();
            match rows_taken_per_corner
                .iter_mut()
                .find(|(taken_corner, _)| taken_corner == corner)
            {
                Some(entry) => entry.1 = rows_taken,
                None => rows_taken_per_corner.push((*corner, rows_taken)),
            }
            geoms[index] = Some(geom);
        }
    }
    popups
        .iter()
        .zip(geoms.into_iter())
        .map(|((placement, wanted_cols, wanted_rows), geom)| {
            geom.unwrap_or_else(|| place_popup_with(*placement, *wanted_cols, *wanted_rows, bounds))
        })
        .collect()
}

fn mouse_event_for_plugin(event: &MouseEvent, line: isize, column: usize) -> Option<Mouse> {
    let (wheel_lines, _) = super::mouse_handler::wheel_steps(event);
    if event.wheel_up {
        return Some(Mouse::ScrollUp(wheel_lines));
    }
    if event.wheel_down {
        return Some(Mouse::ScrollDown(wheel_lines));
    }
    match event.event_type {
        MouseEventType::Press if event.left => Some(Mouse::LeftClick(line, column)),
        MouseEventType::Press if event.right => Some(Mouse::RightClick(line, column)),
        MouseEventType::Motion if event.left => Some(Mouse::Hold(line, column)),
        MouseEventType::Motion if !event.right && !event.middle => Some(Mouse::Hover(line, column)),
        MouseEventType::Release => Some(Mouse::Release(line, column)),
        _ => None,
    }
}

impl Tab {
    pub fn open_popup(
        &mut self,
        client_id: ClientId,
        plugin_id: u32,
        placement: PopupPlacement,
        kind: PopupKind,
        wanted_cols: usize,
        wanted_rows: usize,
        invoked_with: Option<Run>,
        title: String,
    ) -> Result<Vec<u32>> {
        let err_context = || format!("failed to open popup for client {client_id}");
        let replaced = if kind == PopupKind::Menu {
            self.close_menu_popups(client_id)
        } else {
            vec![]
        };
        let mut pane = Box::new(PluginPane::new(
            plugin_id,
            PaneGeom::default(),
            self.senders
                .to_plugin
                .as_ref()
                .with_context(err_context)?
                .clone(),
            title,
            String::new(),
            self.sixel_image_store.clone(),
            self.kitty_image_store.clone(),
            self.terminal_emulator_colors.clone(),
            self.terminal_emulator_color_codes.clone(),
            self.link_handler.clone(),
            self.character_cell_size.clone(),
            vec![client_id],
            self.style,
            invoked_with,
            self.debug,
            self.arrow_fonts,
            self.styled_underlines,
        )) as Box<dyn Pane>;
        pane.set_borderless(true);
        pane.set_content_offset(Offset::default());
        self.popups.entry(client_id).or_default().push(Popup {
            pane,
            placement,
            kind,
            wanted_cols,
            wanted_rows,
            anchor_pane: None,
            held_back: false,
        });
        self.reflow_popups_for_client(client_id, Some(plugin_id));
        self.set_force_render();
        let _ = self.update_input_modes();
        Ok(replaced)
    }
    pub fn popup_plugin_ids_for_client(&self, client_id: ClientId) -> Vec<u32> {
        self.popups
            .get(&client_id)
            .map(|stack| stack.iter().filter_map(|popup| popup.plugin_id()).collect())
            .unwrap_or_default()
    }
    fn popup_with_plugin_id_mut(&mut self, plugin_id: u32) -> Option<&mut Popup> {
        self.popups
            .values_mut()
            .flat_map(|stack| stack.iter_mut())
            .find(|popup| popup.plugin_id() == Some(plugin_id))
    }
    pub fn hold_back_popup(&mut self, plugin_id: u32) {
        if let Some(popup) = self.popup_with_plugin_id_mut(plugin_id) {
            popup.held_back = true;
        }
    }
    pub fn reveal_popup(&mut self, plugin_id: u32) -> bool {
        let Some(popup) = self.popup_with_plugin_id_mut(plugin_id) else {
            return false;
        };
        if !popup.held_back {
            return false;
        }
        popup.held_back = false;
        popup.pane.set_should_render(true);
        popup.pane.render_full_viewport();
        self.set_force_render();
        true
    }
    pub fn set_popup_anchor(&mut self, plugin_id: u32, anchor_pane: Option<PaneId>) {
        if let Some(popup) = self
            .popups
            .values_mut()
            .flat_map(|stack| stack.iter_mut())
            .find(|popup| popup.plugin_id() == Some(plugin_id))
        {
            popup.anchor_pane = anchor_pane;
        }
    }
    fn reflow_popups_for_client(&mut self, client_id: ClientId, always_resize: Option<u32>) {
        let bounds = self.popup_bounds();
        let Some(stack) = self.popups.get_mut(&client_id) else {
            return;
        };
        let wanted: Vec<(PopupPlacement, usize, usize)> = stack
            .iter()
            .map(|popup| (popup.placement, popup.wanted_cols, popup.wanted_rows))
            .collect();
        let geoms = place_popup_stack(&wanted, bounds);
        let mut resized = vec![];
        for (popup, geom) in stack.iter_mut().zip(geoms.into_iter()) {
            let changed = geom != popup.pane.current_geom();
            if changed {
                popup.pane.set_geom(geom);
            }
            if let Some(plugin_id) = popup.plugin_id() {
                if changed || always_resize == Some(plugin_id) {
                    resized.push((
                        plugin_id,
                        popup.pane.get_content_columns(),
                        popup.pane.get_content_rows(),
                    ));
                }
            }
        }
        let any_resized = !resized.is_empty();
        for (plugin_id, cols, rows) in resized {
            let _ = self
                .senders
                .send_to_plugin(PluginInstruction::Resize(plugin_id, cols, rows));
        }
        if any_resized {
            self.set_force_render();
        }
    }
    pub(crate) fn take_info_popups(&mut self, client_id: ClientId) -> Vec<Popup> {
        let Some(stack) = self.popups.get_mut(&client_id) else {
            return vec![];
        };
        let (info, rest): (Vec<Popup>, Vec<Popup>) =
            stack.drain(..).partition(|popup| !popup.takes_focus());
        *stack = rest;
        if stack.is_empty() {
            self.popups.remove(&client_id);
        }
        if !info.is_empty() {
            self.reflow_popups_for_client(client_id, None);
            self.set_force_render();
        }
        info
    }
    pub(crate) fn adopt_popups(&mut self, client_id: ClientId, popups: Vec<Popup>) {
        if popups.is_empty() {
            return;
        }
        let plugin_ids: Vec<u32> = popups
            .iter()
            .filter_map(|popup| popup.plugin_id())
            .collect();
        let stack = self.popups.entry(client_id).or_default();
        let insert_at = stack
            .iter()
            .position(|popup| popup.takes_focus())
            .unwrap_or(stack.len());
        for (offset, popup) in popups.into_iter().enumerate() {
            stack.insert(insert_at + offset, popup);
        }
        for plugin_id in plugin_ids {
            self.reflow_popups_for_client(client_id, Some(plugin_id));
        }
        self.mark_popups_for_full_render();
        self.set_force_render();
        let _ = self.update_input_modes();
    }
    pub fn close_focused_popups(&mut self, client_id: ClientId) -> Vec<u32> {
        self.remove_popups_where(client_id, |popup| popup.takes_focus())
    }
    fn remove_popups_where(
        &mut self,
        client_id: ClientId,
        should_remove: impl Fn(&Popup) -> bool,
    ) -> Vec<u32> {
        let Some(stack) = self.popups.get_mut(&client_id) else {
            return vec![];
        };
        let mut removed = vec![];
        stack.retain(|popup| {
            if should_remove(popup) {
                if let Some(plugin_id) = popup.plugin_id() {
                    removed.push(plugin_id);
                }
                false
            } else {
                true
            }
        });
        if stack.is_empty() {
            self.popups.remove(&client_id);
        }
        if !removed.is_empty() {
            self.reflow_popups_for_client(client_id, None);
            self.set_force_render();
        }
        removed
    }
    pub fn close_popup(&mut self, client_id: ClientId) -> Vec<u32> {
        self.remove_popups_where(client_id, |_| true)
    }
    pub fn close_menu_popups(&mut self, client_id: ClientId) -> Vec<u32> {
        self.remove_popups_where(client_id, |popup| popup.kind == PopupKind::Menu)
    }
    pub fn close_info_popups(&mut self, client_id: ClientId) -> Vec<u32> {
        self.remove_popups_where(client_id, |popup| popup.kind == PopupKind::Info)
    }
    pub fn close_popups_with_missing_anchor(&mut self) -> Vec<(ClientId, u32)> {
        let missing_anchors: Vec<PaneId> = self
            .popups
            .values()
            .flat_map(|stack| stack.iter())
            .filter_map(|popup| popup.anchor_pane)
            .filter(|pane_id| self.get_pane_with_id(*pane_id).is_none())
            .collect();
        if missing_anchors.is_empty() {
            return vec![];
        }
        let client_ids: Vec<ClientId> = self.popups.keys().copied().collect();
        let mut closed = vec![];
        for client_id in client_ids {
            let removed = self.remove_popups_where(client_id, |popup| {
                popup
                    .anchor_pane
                    .map(|pane_id| missing_anchors.contains(&pane_id))
                    .unwrap_or(false)
            });
            closed.extend(removed.into_iter().map(|plugin_id| (client_id, plugin_id)));
        }
        closed
    }
    pub fn close_top_popup(&mut self, client_id: ClientId) -> Option<u32> {
        let top_plugin_id = self.popup_plugin_id(client_id)?;
        self.remove_popups_where(client_id, |popup| popup.plugin_id() == Some(top_plugin_id))
            .into_iter()
            .next()
    }
    pub fn close_popup_with_plugin_id(&mut self, plugin_id: u32) -> Option<ClientId> {
        let client_id = self.popup_client_for_plugin(plugin_id)?;
        self.remove_popups_where(client_id, |popup| popup.plugin_id() == Some(plugin_id));
        Some(client_id)
    }
    pub fn drain_popups(&mut self) -> Vec<(ClientId, u32)> {
        let client_ids: Vec<ClientId> = self.popups.keys().copied().collect();
        client_ids
            .into_iter()
            .flat_map(|client_id| {
                self.close_popup(client_id)
                    .into_iter()
                    .map(move |plugin_id| (client_id, plugin_id))
            })
            .collect()
    }
    pub fn popup_client_for_plugin(&self, plugin_id: u32) -> Option<ClientId> {
        self.popups
            .iter()
            .find(|(_, stack)| {
                stack
                    .iter()
                    .any(|popup| popup.plugin_id() == Some(plugin_id))
            })
            .map(|(client_id, _)| *client_id)
    }
    pub fn popup_plugin_id(&self, client_id: ClientId) -> Option<u32> {
        self.top_focused_popup(client_id)
            .and_then(|popup| popup.plugin_id())
    }
    fn top_focused_popup(&self, client_id: ClientId) -> Option<&Popup> {
        self.popups
            .get(&client_id)
            .and_then(|stack| stack.iter().rev().find(|popup| popup.takes_focus()))
    }
    pub fn popup_kind_of_plugin(&self, plugin_id: u32) -> Option<PopupKind> {
        self.popups
            .values()
            .flat_map(|stack| stack.iter())
            .find(|popup| popup.plugin_id() == Some(plugin_id))
            .map(|popup| popup.kind)
    }
    #[cfg(test)]
    pub fn has_popup_for_client(&self, client_id: ClientId) -> bool {
        self.popups
            .get(&client_id)
            .map(|stack| !stack.is_empty())
            .unwrap_or(false)
    }
    pub fn has_focused_popup_for_client(&self, client_id: ClientId) -> bool {
        self.top_focused_popup(client_id).is_some()
    }
    pub fn focused_popup_takes_all_keys(&self, client_id: ClientId) -> bool {
        self.top_focused_popup(client_id)
            .map(|popup| popup.kind == PopupKind::Modal)
            .unwrap_or(false)
    }
    #[cfg(test)]
    pub fn info_popup_geoms(&self, client_id: ClientId) -> Vec<(u32, PaneGeom)> {
        self.popups
            .get(&client_id)
            .map(|stack| {
                stack
                    .iter()
                    .filter(|popup| !popup.takes_focus())
                    .filter_map(|popup| {
                        popup
                            .plugin_id()
                            .map(|plugin_id| (plugin_id, popup.pane.current_geom()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    #[cfg(test)]
    pub fn popup_count_for_client(&self, client_id: ClientId) -> usize {
        self.popups
            .get(&client_id)
            .map(|stack| stack.len())
            .unwrap_or(0)
    }
    #[cfg(test)]
    pub fn popup_geom(&self, client_id: ClientId) -> Option<PaneGeom> {
        self.top_focused_popup(client_id)
            .map(|popup| popup.pane.current_geom())
    }
    pub fn popup_geoms(&self, client_id: ClientId) -> Vec<PaneGeom> {
        self.popups
            .get(&client_id)
            .map(|stack| {
                let mut drawing_order: Vec<&Popup> =
                    stack.iter().filter(|popup| !popup.held_back).collect();
                drawing_order.sort_by_key(|popup| popup.takes_focus());
                drawing_order
                    .into_iter()
                    .map(|popup| popup.pane.current_geom())
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn send_key_to_popup(
        &mut self,
        client_id: ClientId,
        key_with_modifier: KeyWithModifier,
        raw_bytes: Vec<u8>,
        is_kitty_keyboard_protocol: bool,
    ) -> bool {
        let Some(plugin_id) = self.popup_plugin_id(client_id) else {
            return false;
        };
        let adjusted_input = match self.popup_pane_mut(plugin_id) {
            Some(pane) => pane.adjust_input_to_terminal(
                &Some(key_with_modifier),
                raw_bytes,
                is_kitty_keyboard_protocol,
                Some(client_id),
            ),
            None => return false,
        };
        match adjusted_input {
            Some(AdjustedInput::WriteKeyToPlugin(key_with_modifier)) => {
                let _ = self.senders.send_to_plugin(PluginInstruction::Update(vec![(
                    Some(plugin_id),
                    Some(client_id),
                    Event::Key(key_with_modifier),
                )]));
            },
            Some(AdjustedInput::PermissionRequestResult(permissions, status)) => {
                if let Some(pane) = self.popup_pane_mut(plugin_id) {
                    pane.request_permissions_from_user(None);
                }
                let _ = self
                    .senders
                    .send_to_plugin(PluginInstruction::PermissionRequestResult(
                        plugin_id,
                        Some(client_id),
                        permissions,
                        status,
                        None,
                    ));
                self.set_force_render();
            },
            _ => {},
        }
        true
    }
    pub fn set_popup_kind(&mut self, plugin_id: u32, kind: PopupKind) -> Option<ClientId> {
        let client_id = self.popup_client_for_plugin(plugin_id)?;
        let stack = self.popups.get_mut(&client_id)?;
        let position = stack
            .iter()
            .position(|popup| popup.plugin_id() == Some(plugin_id))?;
        let mut popup = stack.remove(position);
        popup.kind = kind;
        popup.pane.set_should_render(true);
        popup.pane.render_full_viewport();
        stack.push(popup);
        self.reflow_popups_for_client(client_id, None);
        self.mark_popups_for_full_render();
        self.set_force_render();
        let _ = self.update_input_modes();
        Some(client_id)
    }
    pub(crate) fn take_popup_pane(&mut self, plugin_id: u32) -> Option<(ClientId, Box<dyn Pane>)> {
        let client_id = self.popup_client_for_plugin(plugin_id)?;
        let stack = self.popups.get_mut(&client_id)?;
        let position = stack
            .iter()
            .position(|popup| popup.plugin_id() == Some(plugin_id))?;
        let popup = stack.remove(position);
        if stack.is_empty() {
            self.popups.remove(&client_id);
        }
        self.reflow_popups_for_client(client_id, None);
        self.mark_popups_for_full_render();
        self.set_force_render();
        let _ = self.update_input_modes();
        Some((client_id, popup.pane))
    }
    pub fn popup_to_floating_pane(
        &mut self,
        plugin_id: u32,
        coordinates: Option<zellij_utils::data::FloatingPaneCoordinates>,
    ) -> Result<Option<ClientId>> {
        let Some((client_id, mut pane)) = self.take_popup_pane(plugin_id) else {
            return Ok(None);
        };
        pane.set_borderless(false);
        pane.set_content_offset(Offset::frame(1));
        pane.set_should_render(true);
        pane.render_full_viewport();
        if !self.are_floating_panes_visible() {
            self.show_floating_panes();
        }
        self.add_floating_pane(
            pane,
            PaneId::Plugin(plugin_id),
            coordinates,
            true,
            Some(client_id),
        )?;
        self.set_force_render();
        Ok(Some(client_id))
    }
    pub fn find_popup_plugin(
        &self,
        run_plugin_or_alias: &zellij_utils::input::layout::RunPluginOrAlias,
    ) -> Option<(ClientId, u32)> {
        self.popups.iter().find_map(|(client_id, stack)| {
            stack.iter().find_map(|popup| {
                let plugin_id = popup.plugin_id()?;
                if run_plugin_or_alias.is_equivalent_to_run(popup.pane.invoked_with()) {
                    Some((*client_id, plugin_id))
                } else {
                    None
                }
            })
        })
    }
    pub fn has_popup_plugin(&self, plugin_id: u32) -> bool {
        self.popup_client_for_plugin(plugin_id).is_some()
    }
    pub fn popup_pane_mut(&mut self, plugin_id: u32) -> Option<&mut Box<dyn Pane>> {
        self.popups
            .values_mut()
            .flat_map(|stack| stack.iter_mut())
            .find(|popup| popup.plugin_id() == Some(plugin_id))
            .map(|popup| &mut popup.pane)
    }
    pub fn resize_popup(&mut self, plugin_id: u32, wanted_cols: usize, wanted_rows: usize) {
        let Some(client_id) = self.popup_client_for_plugin(plugin_id) else {
            return;
        };
        if let Some(popup) = self.popups.get_mut(&client_id).and_then(|stack| {
            stack
                .iter_mut()
                .find(|popup| popup.plugin_id() == Some(plugin_id))
        }) {
            popup.wanted_cols = wanted_cols;
            popup.wanted_rows = wanted_rows;
        }
        self.reflow_popups_for_client(client_id, None);
    }
    pub fn popup_bounds(&self) -> Viewport {
        let viewport = *self.viewport.borrow();
        if viewport.has_positive_size() {
            viewport
        } else {
            let display_area = *self.display_area.borrow();
            Viewport {
                x: 0,
                y: 0,
                rows: display_area.rows,
                cols: display_area.cols,
            }
        }
    }
    pub fn relayout_popups(&mut self) {
        let client_ids: Vec<ClientId> = self.popups.keys().copied().collect();
        for client_id in client_ids {
            self.reflow_popups_for_client(client_id, None);
        }
    }
    pub fn mark_popups_for_full_render(&mut self) {
        for popup in self.popups.values_mut().flat_map(|stack| stack.iter_mut()) {
            popup.pane.set_should_render(true);
            popup.pane.render_full_viewport();
        }
    }
    pub fn handle_popup_mouse_event(
        &mut self,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Option<PopupMouseOutcome> {
        match self.top_focused_popup(client_id) {
            Some(_) => self.handle_focused_popup_mouse_event(event, client_id),
            None => self.handle_info_popup_mouse_event(event, client_id),
        }
    }
    pub fn popup_for_location(&self, client_id: ClientId, location: &str) -> Option<(u32, bool)> {
        self.popups.get(&client_id)?.iter().find_map(|popup| {
            let matches = match popup.pane.invoked_with() {
                Some(Run::Plugin(run_plugin_or_alias)) => {
                    run_plugin_or_alias.location_string().contains(location)
                },
                _ => false,
            };
            if matches {
                popup
                    .plugin_id()
                    .map(|plugin_id| (plugin_id, popup.takes_focus()))
            } else {
                None
            }
        })
    }
    pub fn unfocused_popup_plugin_ids(&self, client_id: ClientId) -> Vec<u32> {
        self.popups
            .get(&client_id)
            .map(|stack| {
                stack
                    .iter()
                    .filter(|popup| !popup.takes_focus())
                    .filter_map(|popup| popup.plugin_id())
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn clear_hover_under_popups(&mut self, client_id: ClientId) -> bool {
        super::mouse_handler::clear_hover_for_client(self, client_id)
    }
    fn handle_focused_popup_mouse_event(
        &mut self,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Option<PopupMouseOutcome> {
        let popup = self.top_focused_popup(client_id)?;
        let geom = popup.pane.current_geom();
        let plugin_id = match popup.plugin_id() {
            Some(plugin_id) => plugin_id,
            None => return Some(PopupMouseOutcome::Consumed),
        };
        let inside = geom.contains(&event.position);
        let is_press = event.event_type == MouseEventType::Press
            && (event.left || event.right || event.middle);
        if !inside && is_press {
            return match popup.kind {
                PopupKind::Menu => Some(PopupMouseOutcome::CloseRequested),
                PopupKind::Prompt | PopupKind::Modal | PopupKind::Info => {
                    Some(PopupMouseOutcome::Consumed)
                },
            };
        }
        let (line, column) = if inside {
            let relative = popup.pane.relative_position(&event.position);
            (relative.line.0, relative.column.0)
        } else {
            (-1, 0)
        };
        let is_wheel = event.wheel_up || event.wheel_down;
        if is_wheel && !inside {
            return Some(PopupMouseOutcome::Consumed);
        }
        if let Some(mouse) = mouse_event_for_plugin(event, line, column) {
            self.send_mouse_to_popup_plugin(plugin_id, client_id, mouse);
        }
        Some(PopupMouseOutcome::Consumed)
    }
    fn handle_info_popup_mouse_event(
        &mut self,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Option<PopupMouseOutcome> {
        let hit = self.popups.get(&client_id).and_then(|stack| {
            stack.iter().rev().find_map(|popup| {
                if popup.takes_focus() || !popup.pane.current_geom().contains(&event.position) {
                    return None;
                }
                popup
                    .plugin_id()
                    .map(|plugin_id| (plugin_id, popup.pane.relative_position(&event.position)))
            })
        });
        if event.event_type == MouseEventType::Motion {
            let current = hit.as_ref().map(|(plugin_id, _)| *plugin_id);
            let previous = self.popup_hover_plugin_id.get(&client_id).copied();
            if let Some(previous) = previous.filter(|previous| Some(*previous) != current) {
                self.send_mouse_to_popup_plugin(previous, client_id, Mouse::Hover(-1, 0));
                self.popup_hover_plugin_id.remove(&client_id);
            }
            if let Some(current) = current {
                self.popup_hover_plugin_id.insert(client_id, current);
            }
        }
        let (plugin_id, relative) = hit?;
        if let Some(mouse) = mouse_event_for_plugin(event, relative.line.0, relative.column.0) {
            self.send_mouse_to_popup_plugin(plugin_id, client_id, mouse);
        }
        Some(PopupMouseOutcome::Consumed)
    }
    fn send_mouse_to_popup_plugin(&self, plugin_id: u32, client_id: ClientId, mouse: Mouse) {
        let _ = self.senders.send_to_plugin(PluginInstruction::Update(vec![(
            Some(plugin_id),
            Some(client_id),
            Event::Mouse(mouse),
        )]));
    }
    pub fn scroll_top_popup(&self, client_id: ClientId, up: bool, lines: usize) -> bool {
        let Some(plugin_id) = self.popup_plugin_id(client_id) else {
            return false;
        };
        let mouse = if up {
            Mouse::ScrollUp(lines)
        } else {
            Mouse::ScrollDown(lines)
        };
        self.send_mouse_to_popup_plugin(plugin_id, client_id, mouse);
        true
    }
    pub(crate) fn render_popups(&mut self, output: &mut Output, force: bool) -> Result<()> {
        let err_context = || "failed to render popups".to_string();
        let connected_clients: Vec<ClientId> =
            { self.connected_clients.borrow().iter().copied().collect() };
        for (client_id, stack) in self.popups.iter_mut() {
            if !connected_clients.contains(client_id) {
                continue;
            }
            let mut drawing_order: Vec<&mut Popup> =
                stack.iter_mut().filter(|popup| !popup.held_back).collect();
            drawing_order.sort_by_key(|popup| popup.takes_focus());
            for (layer, popup) in drawing_order.into_iter().enumerate() {
                if force {
                    popup.pane.set_should_render(true);
                    popup.pane.render_full_viewport();
                }
                if let Some((character_chunks, raw_vte_output, _sixel_chunks, _kitty_chunks)) =
                    popup
                        .pane
                        .render(Some(*client_id))
                        .with_context(err_context)?
                {
                    output
                        .add_character_chunks_to_client(
                            *client_id,
                            character_chunks,
                            Some(POPUP_Z_INDEX + layer),
                        )
                        .with_context(err_context)?;
                    if let Some(raw_vte_output) = raw_vte_output {
                        output.add_post_vte_instruction_to_client(
                            *client_id,
                            &format!(
                                "\u{1b}[{};{}H\u{1b}[m{}",
                                popup.pane.y() + 1,
                                popup.pane.x() + 1,
                                raw_vte_output
                            ),
                        );
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) fn set_popup_covers(&self, output: &mut Output) {
        let connected_clients: Vec<ClientId> =
            { self.connected_clients.borrow().iter().copied().collect() };
        for client_id in connected_clients {
            output.set_popup_cover(client_id, self.popup_geoms(client_id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn popup_in(corner: PopupCorner, rows: usize) -> (PopupPlacement, usize, usize) {
        (PopupPlacement::Corner(corner), 20, rows)
    }

    #[test]
    fn corner_popups_sit_in_their_corner() {
        let geoms = place_popup_stack(
            &[
                popup_in(PopupCorner::TopRight, 4),
                popup_in(PopupCorner::TopLeft, 4),
                popup_in(PopupCorner::BottomRight, 4),
                popup_in(PopupCorner::BottomLeft, 4),
            ],
            viewport_between_bars(),
        );
        let positions: Vec<(usize, usize)> = geoms.iter().map(|g| (g.x, g.y)).collect();
        assert_eq!(positions, vec![(60, 1), (0, 1), (60, 15), (0, 15)]);
    }

    #[test]
    fn popups_in_the_same_corner_stack_with_the_newest_nearest_the_corner() {
        let geoms = place_popup_stack(
            &[
                popup_in(PopupCorner::TopRight, 4),
                popup_in(PopupCorner::TopRight, 3),
                popup_in(PopupCorner::TopRight, 5),
            ],
            viewport_between_bars(),
        );
        let lines: Vec<usize> = geoms.iter().map(|g| g.y).collect();
        assert_eq!(lines, vec![9, 6, 1]);
    }

    #[test]
    fn bottom_corner_popups_stack_upwards() {
        let geoms = place_popup_stack(
            &[
                popup_in(PopupCorner::BottomLeft, 4),
                popup_in(PopupCorner::BottomLeft, 3),
            ],
            viewport_between_bars(),
        );
        let lines: Vec<usize> = geoms.iter().map(|g| g.y).collect();
        assert_eq!(lines, vec![12, 16]);
    }

    #[test]
    fn non_corner_popups_are_not_moved_by_corner_stacking() {
        let geoms = place_popup_stack(
            &[
                (PopupPlacement::At(Position::new(5, 10)), 20, 8),
                popup_in(PopupCorner::TopLeft, 4),
            ],
            screen(),
        );
        assert_eq!((geoms[0].x, geoms[0].y), (10, 5));
        assert_eq!((geoms[1].x, geoms[1].y), (0, 0));
    }

    fn screen() -> Viewport {
        Viewport {
            x: 0,
            y: 0,
            rows: 20,
            cols: 80,
        }
    }

    fn viewport_between_bars() -> Viewport {
        Viewport {
            x: 0,
            y: 1,
            rows: 18,
            cols: 80,
        }
    }

    #[test]
    fn popup_opened_from_the_top_bar_appears_below_it() {
        let geom = place_popup(Position::new(0, 10), 20, 8, viewport_between_bars());
        assert_eq!((geom.x, geom.y), (10, 1));
    }

    #[test]
    fn popup_opened_from_the_bottom_bar_appears_above_it() {
        let geom = place_popup(Position::new(19, 10), 20, 8, viewport_between_bars());
        assert_eq!((geom.x, geom.y), (10, 11));
        assert_eq!(geom.y + geom.rows.as_usize(), 19);
    }

    #[test]
    fn popup_taller_than_the_space_between_bars_is_shrunk_to_it() {
        let geom = place_popup(Position::new(5, 10), 20, 30, viewport_between_bars());
        assert_eq!((geom.y, geom.rows.as_usize()), (1, 18));
    }

    #[test]
    fn popup_opens_below_right_of_the_pointer_when_there_is_room() {
        let geom = place_popup(Position::new(5, 10), 20, 8, screen());
        assert_eq!((geom.x, geom.y), (10, 5));
        assert_eq!((geom.cols.as_usize(), geom.rows.as_usize()), (20, 8));
    }

    #[test]
    fn popup_is_moved_left_at_the_right_edge_instead_of_shrinking() {
        let geom = place_popup(Position::new(5, 75), 20, 8, screen());
        assert_eq!((geom.x, geom.y), (60, 5));
        assert_eq!((geom.cols.as_usize(), geom.rows.as_usize()), (20, 8));
    }

    #[test]
    fn popup_is_moved_up_at_the_bottom_edge_instead_of_shrinking() {
        let geom = place_popup(Position::new(18, 10), 20, 8, screen());
        assert_eq!((geom.x, geom.y), (10, 12));
        assert_eq!((geom.cols.as_usize(), geom.rows.as_usize()), (20, 8));
    }

    #[test]
    fn popup_is_moved_left_and_up_in_the_bottom_right_corner() {
        let geom = place_popup(Position::new(19, 79), 20, 8, screen());
        assert_eq!((geom.x, geom.y), (60, 12));
    }

    #[test]
    fn popup_that_exactly_fits_is_not_moved() {
        let geom = place_popup(Position::new(12, 60), 20, 8, screen());
        assert_eq!((geom.x, geom.y), (60, 12));
    }

    #[test]
    fn popup_larger_than_the_screen_is_shrunk_to_the_screen() {
        let geom = place_popup(Position::new(3, 3), 100, 30, screen());
        assert_eq!((geom.x, geom.y), (0, 0));
        assert_eq!((geom.cols.as_usize(), geom.rows.as_usize()), (80, 20));
    }
}

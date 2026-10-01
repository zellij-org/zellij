use super::{Pane, Tab};
use crate::output::Output;
use crate::panes::{PaneId, PluginPane};
use crate::plugins::PluginInstruction;
use crate::ClientId;
use zellij_utils::data::{Event, Mouse};
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupKind {
    Menu,
    Prompt,
}

pub(crate) struct Popup {
    pub pane: Box<dyn Pane>,
    pub placement: PopupPlacement,
    pub kind: PopupKind,
    pub wanted_cols: usize,
    pub wanted_rows: usize,
}

impl Popup {
    fn plugin_id(&self) -> Option<u32> {
        match self.pane.pid() {
            PaneId::Plugin(plugin_id) => Some(plugin_id),
            PaneId::Terminal(_) => None,
        }
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
    }
}

fn mouse_event_for_plugin(event: &MouseEvent, line: isize, column: usize) -> Option<Mouse> {
    if event.wheel_up {
        return Some(Mouse::ScrollUp(3));
    }
    if event.wheel_down {
        return Some(Mouse::ScrollDown(3));
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
        let bounds = self.popup_bounds();
        pane.set_borderless(true);
        pane.set_content_offset(Offset::default());
        pane.set_geom(place_popup_with(placement, wanted_cols, wanted_rows, bounds));
        self.senders
            .send_to_plugin(PluginInstruction::Resize(
                plugin_id,
                pane.get_content_columns(),
                pane.get_content_rows(),
            ))
            .with_context(err_context)?;
        self.popups.entry(client_id).or_default().push(Popup {
            pane,
            placement,
            kind,
            wanted_cols,
            wanted_rows,
        });
        self.set_force_render();
        Ok(replaced)
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
    pub fn close_top_popup(&mut self, client_id: ClientId) -> Option<u32> {
        let top_plugin_id = self.popup_plugin_id(client_id)?;
        self.remove_popups_where(client_id, |popup| {
            popup.plugin_id() == Some(top_plugin_id)
        })
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
            .find(|(_, stack)| stack.iter().any(|popup| popup.plugin_id() == Some(plugin_id)))
            .map(|(client_id, _)| *client_id)
    }
    pub fn popup_plugin_id(&self, client_id: ClientId) -> Option<u32> {
        self.popups
            .get(&client_id)
            .and_then(|stack| stack.last())
            .and_then(|popup| popup.plugin_id())
    }
    pub fn popup_kind_of_plugin(&self, plugin_id: u32) -> Option<PopupKind> {
        self.popups
            .values()
            .flat_map(|stack| stack.iter())
            .find(|popup| popup.plugin_id() == Some(plugin_id))
            .map(|popup| popup.kind)
    }
    pub fn has_popup_for_client(&self, client_id: ClientId) -> bool {
        self.popups
            .get(&client_id)
            .map(|stack| !stack.is_empty())
            .unwrap_or(false)
    }
    #[cfg(test)]
    pub fn popup_count_for_client(&self, client_id: ClientId) -> usize {
        self.popups.get(&client_id).map(|stack| stack.len()).unwrap_or(0)
    }
    #[cfg(test)]
    pub fn popup_geom(&self, client_id: ClientId) -> Option<PaneGeom> {
        self.popups
            .get(&client_id)
            .and_then(|stack| stack.last())
            .map(|popup| popup.pane.current_geom())
    }
    pub fn popup_geoms(&self, client_id: ClientId) -> Vec<PaneGeom> {
        self.popups
            .get(&client_id)
            .map(|stack| stack.iter().map(|popup| popup.pane.current_geom()).collect())
            .unwrap_or_default()
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
        let bounds = self.popup_bounds();
        let mut resized = None;
        if let Some(popup) = self
            .popups
            .values_mut()
            .flat_map(|stack| stack.iter_mut())
            .find(|popup| popup.plugin_id() == Some(plugin_id))
        {
            popup.wanted_cols = wanted_cols;
            popup.wanted_rows = wanted_rows;
            let new_geom = place_popup_with(popup.placement, wanted_cols, wanted_rows, bounds);
            if new_geom != popup.pane.current_geom() {
                popup.pane.set_geom(new_geom);
                resized = Some((
                    popup.pane.get_content_columns(),
                    popup.pane.get_content_rows(),
                ));
            }
        }
        if let Some((cols, rows)) = resized {
            let _ = self
                .senders
                .send_to_plugin(PluginInstruction::Resize(plugin_id, cols, rows));
            self.set_force_render();
        }
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
        let bounds = self.popup_bounds();
        let mut resized = vec![];
        for popup in self.popups.values_mut().flat_map(|stack| stack.iter_mut()) {
            let new_geom =
                place_popup_with(popup.placement, popup.wanted_cols, popup.wanted_rows, bounds);
            if new_geom != popup.pane.current_geom() {
                popup.pane.set_geom(new_geom);
                if let Some(plugin_id) = popup.plugin_id() {
                    resized.push((
                        plugin_id,
                        popup.pane.get_content_columns(),
                        popup.pane.get_content_rows(),
                    ));
                }
            }
        }
        for (plugin_id, cols, rows) in resized {
            let _ = self
                .senders
                .send_to_plugin(PluginInstruction::Resize(plugin_id, cols, rows));
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
        let popup = self.popups.get(&client_id).and_then(|stack| stack.last())?;
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
                PopupKind::Prompt => Some(PopupMouseOutcome::Consumed),
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
            let _ = self.senders.send_to_plugin(PluginInstruction::Update(vec![(
                Some(plugin_id),
                Some(client_id),
                Event::Mouse(mouse),
            )]));
        }
        Some(PopupMouseOutcome::Consumed)
    }
    pub(crate) fn render_popups(&mut self, output: &mut Output, force: bool) -> Result<()> {
        let err_context = || "failed to render popups".to_string();
        let connected_clients: Vec<ClientId> =
            { self.connected_clients.borrow().iter().copied().collect() };
        for (client_id, stack) in self.popups.iter_mut() {
            if !connected_clients.contains(client_id) {
                continue;
            }
            for (layer, popup) in stack.iter_mut().enumerate() {
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

use super::Screen;
use crate::pane_mirror::live_view::{parse_release_key, release_key_text_for, DragButton};
use crate::pane_mirror::{
    LiveViewKeyOutcome, LiveViewRect, LiveViewReport, LiveViewSlot, MirrorEventRoute, MirrorView,
};
use crate::panes::PaneId;
use crate::plugins::PluginInstruction;
use crate::route::NotificationEnd;
use crate::screen::ScreenInstruction;
use crate::ClientId;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;
use zellij_utils::data::{InputMode, KeyWithModifier, LiveView, LiveViewRelease, PaneAddress};
use zellij_utils::input::mouse::{MouseEvent, MouseEventType};
use zellij_utils::pane_mirror::MirrorTransportEvent;
use zellij_utils::position::Position;

pub(crate) const NO_LIVE_VIEW: &str = "The focused pane has no live view";
pub(crate) const LIVE_VIEW_IS_VIEW_ONLY: &str = "The live view is view-only";

fn target_of(view: &LiveView) -> Option<PaneAddress> {
    if view.target.session.is_empty() {
        None
    } else {
        Some(view.target.clone())
    }
}

impl Screen {
    fn live_view_route(&self) -> MirrorEventRoute {
        let senders = self.bus.senders.clone();
        Arc::new(move |view_id, generation, event| {
            let _ = senders.send_to_screen(ScreenInstruction::MirrorTransportEvent {
                view_id,
                generation,
                event,
            });
        })
    }

    fn live_view_content_area(&self, plugin_pane_id: u32) -> Option<LiveViewRect> {
        self.tabs
            .values()
            .find_map(|tab| tab.live_view_content_area(plugin_pane_id))
    }

    fn client_focuses_plugin_pane(&self, client_id: ClientId, plugin_pane_id: u32) -> bool {
        self.get_active_tab(client_id)
            .ok()
            .and_then(|tab| tab.live_view_focus(client_id))
            == Some(PaneId::Plugin(plugin_pane_id))
    }

    fn live_view_takes_input(&self, client_id: ClientId, plugin_pane_id: u32) -> bool {
        self.client_focuses_plugin_pane(client_id, plugin_pane_id)
            && self
                .live_view_content_area(plugin_pane_id)
                .map(|content| content.cols > 0 && content.rows > 0)
                .unwrap_or(false)
    }

    pub(crate) fn release_live_view_if_unreachable(&mut self, client_id: ClientId) {
        let Some((plugin_pane_id, _)) = self.live_views.borrow().captured_by(client_id) else {
            return;
        };
        if self.live_view_takes_input(client_id, plugin_pane_id) {
            return;
        }
        self.live_views
            .borrow_mut()
            .release(&(plugin_pane_id, client_id), LiveViewRelease::FocusLost);
        self.after_live_view_capture_change();
    }

    fn client_is_in_a_prompt(&self, client_id: ClientId) -> bool {
        matches!(
            self.get_client_input_mode(client_id),
            Some(InputMode::RenameTab | InputMode::RenamePane | InputMode::EnterSearch)
        )
    }

    fn client_may_have_live_views(&self, client_id: ClientId) -> bool {
        !self.watcher_clients.contains_key(&client_id)
            && self.connected_clients.borrow().contains_key(&client_id)
    }

    fn redraw_plugin_pane(&mut self, plugin_pane_id: u32) {
        for tab in self.tabs.values_mut() {
            tab.mark_plugin_pane_for_full_render(plugin_pane_id);
        }
    }

    fn live_view_release_key_text(&self, client_id: ClientId) -> String {
        let live_views = self.live_views.borrow();
        if let Some(text) = live_views.release_key_text.clone() {
            return text;
        }
        let bound = live_views
            .captures
            .lock()
            .map(|captures| captures.bound_keys_of(client_id))
            .unwrap_or_default();
        release_key_text_for(bound)
    }

    fn send_live_view_report(
        &self,
        plugin_pane_id: u32,
        client_id: ClientId,
        report: LiveViewReport,
    ) {
        let release_key = self.live_view_release_key_text(client_id);
        let _ = self
            .bus
            .senders
            .send_to_plugin(PluginInstruction::Update(vec![(
                Some(plugin_pane_id),
                Some(client_id),
                report.into_event(release_key),
            )]));
    }

    pub(crate) fn set_live_view_release_key(&mut self, release_key: Option<String>) {
        let configured = parse_release_key(release_key.as_deref());
        {
            let mut live_views = self.live_views.borrow_mut();
            live_views.set_release_key(configured);
            live_views.styled_underlines = self.styled_underlines;
        }
        self.report_live_views();
        self.announce_live_view_captures();
    }

    fn note_live_view_release_keys(&mut self) {
        let clients = self.live_views.borrow().clients();
        let release_keys: BTreeMap<ClientId, String> = clients
            .into_iter()
            .map(|client_id| (client_id, self.live_view_release_key_text(client_id)))
            .collect();
        self.live_views.borrow_mut().note_release_keys(release_keys);
    }

    pub(crate) fn set_live_view(
        &mut self,
        plugin_pane_id: u32,
        client_id: ClientId,
        view: Option<LiveView>,
    ) {
        let key = (plugin_pane_id, client_id);
        let Some(view) = view else {
            let removed = self.live_views.borrow_mut().remove(&key);
            if let Some(slot) = removed {
                let released_by = if slot.input_captured {
                    Some(LiveViewRelease::Cleared)
                } else {
                    slot.released_by
                };
                let view_only = slot.view.is_view_only();
                drop(slot);
                self.announce_live_view_captures();
                self.send_live_view_report(
                    plugin_pane_id,
                    client_id,
                    LiveViewReport::cleared(released_by, view_only),
                );
                self.redraw_plugin_pane(plugin_pane_id);
                self.schedule_mirror_tick();
                let _ = self.render(None);
            }
            return;
        };
        if !self.client_may_have_live_views(client_id) {
            return;
        }
        let rect = LiveViewRect::new(view.x, view.y, view.cols, view.rows);
        let content = self.live_view_content_area(plugin_pane_id);
        let viewer_size = content
            .map(|content| rect.clamped_to(content.cols, content.rows))
            .unwrap_or(rect)
            .size();
        let target = target_of(&view);
        let default_resize_allowed = self.pane_mirrors.resizes_source_pane;
        let mut viewer_sessions = self.nested_ancestry.clone();
        viewer_sessions.push(self.session_name.clone());
        let rect_moved = {
            let mut live_views = self.live_views.borrow_mut();
            let reusable = live_views
                .get(&key)
                .map(|slot| slot.view.is_view_only() == view.view_only)
                .unwrap_or(false);
            if reusable {
                let slot = live_views.get_mut(&key).expect("the live view exists");
                let moved = slot.rect != rect;
                slot.rect = rect;
                if view.resize_allowed.is_some()
                    && view.resize_allowed != slot.requested_resize_allowed
                {
                    slot.requested_resize_allowed = view.resize_allowed;
                    if let Some(resize_allowed) = view.resize_allowed {
                        slot.view.set_resize_allowed(resize_allowed);
                    }
                    slot.force_report();
                }
                slot.view.set_viewer_sessions(viewer_sessions);
                if slot.view.target() != target.as_ref() {
                    live_views.cut_drag_of_view(&key);
                    let slot = live_views.get_mut(&key).expect("the live view exists");
                    slot.release(LiveViewRelease::Cleared);
                    let update = slot.view.set_target(target, viewer_size);
                    slot.redraw(update.redraw);
                } else {
                    slot.view.resized(viewer_size);
                }
                moved
            } else {
                let released_by = live_views
                    .remove(&key)
                    .and_then(|old| old.input_captured.then_some(LiveViewRelease::Cleared));
                let view_id = live_views.allocate_view_id();
                let mut mirror_view = MirrorView::new(
                    view_id,
                    self.mirror_connector.clone(),
                    self.live_view_route(),
                    view.view_only,
                    view.resize_allowed.unwrap_or(default_resize_allowed),
                );
                mirror_view.set_viewer_sessions(viewer_sessions);
                mirror_view.set_target(target, viewer_size);
                let mut slot = LiveViewSlot::new(mirror_view, rect, view.resize_allowed);
                slot.released_by = released_by;
                live_views.insert(key, slot);
                true
            }
        };
        self.announce_live_view_captures();
        if rect_moved {
            self.redraw_plugin_pane(plugin_pane_id);
        }
        self.report_live_views();
        self.schedule_mirror_tick();
        let _ = self.render(None);
    }

    pub(crate) fn set_live_view_input(
        &mut self,
        plugin_pane_id: u32,
        client_id: ClientId,
        capture: bool,
    ) {
        let key = (plugin_pane_id, client_id);
        if capture {
            let has_focus = self.client_focuses_plugin_pane(client_id, plugin_pane_id);
            let refused = self
                .live_views
                .borrow_mut()
                .start_capture(key, has_focus)
                .is_err();
            if refused {
                let report = self
                    .live_views
                    .borrow_mut()
                    .get_mut(&key)
                    .and_then(|slot| slot.report_if_changed())
                    .unwrap_or_else(|| {
                        LiveViewReport::cleared(Some(LiveViewRelease::NotAllowed), false)
                    });
                self.send_live_view_report(plugin_pane_id, client_id, report);
            }
        } else {
            self.live_views
                .borrow_mut()
                .release(&key, LiveViewRelease::Plugin);
        }
        self.after_live_view_capture_change();
    }

    pub(crate) fn release_live_view_input(&mut self, client_id: ClientId, reason: LiveViewRelease) {
        if self
            .live_views
            .borrow_mut()
            .release_client(client_id, reason)
        {
            self.after_live_view_capture_change();
        }
    }

    fn release_live_view_input_after_the_click(&mut self, client_id: ClientId) {
        let released = self
            .live_views
            .borrow_mut()
            .release_client(client_id, LiveViewRelease::ClickOutside);
        if released {
            self.announce_live_view_captures();
            let _ = self
                .bus
                .senders
                .send_to_screen(ScreenInstruction::ReportLiveViewCaptureChange);
        }
    }

    pub(crate) fn after_live_view_capture_change(&mut self) {
        self.report_live_views();
        self.announce_live_view_captures();
        self.apply_mirror_pins();
        self.schedule_mirror_tick();
        let _ = self.render(None);
    }

    fn forget_live_views(&mut self, keys: Vec<(u32, ClientId)>) {
        if keys.is_empty() {
            return;
        }
        {
            let mut live_views = self.live_views.borrow_mut();
            for key in &keys {
                live_views.remove(key);
            }
        }
        self.announce_live_view_captures();
        self.schedule_mirror_tick();
    }

    pub(crate) fn clear_live_views_of_plugin(&mut self, plugin_pane_id: u32) {
        let keys = self.live_views.borrow().keys_of_plugin(plugin_pane_id);
        let had_views = !keys.is_empty();
        self.forget_live_views(keys);
        if had_views {
            self.redraw_plugin_pane(plugin_pane_id);
            let _ = self.render(None);
        }
    }

    pub(crate) fn forget_live_views_of_closed_pane(&mut self, plugin_pane_id: u32) {
        if self.live_view_content_area(plugin_pane_id).is_some() {
            return;
        }
        let keys = self.live_views.borrow().keys_of_plugin(plugin_pane_id);
        self.forget_live_views(keys);
    }

    pub(crate) fn clear_live_views_of_client(&mut self, client_id: ClientId) {
        let keys = self.live_views.borrow().keys_of_client(client_id);
        self.forget_live_views(keys);
    }

    pub(crate) fn handle_live_view_transport_event(
        &mut self,
        view_id: u32,
        generation: u64,
        event: MirrorTransportEvent,
    ) {
        let handled = {
            let mut live_views = self.live_views.borrow_mut();
            let Some(key) = live_views.key_of_view(view_id) else {
                return;
            };
            let slot = live_views.get_mut(&key).expect("the live view exists");
            match slot.view.handle_event(generation, event) {
                Some(update) => {
                    slot.redraw(update.redraw);
                    Some(update.state_changed)
                },
                None => None,
            }
        };
        if let Some(state_changed) = handled {
            if state_changed {
                self.report_live_views();
            }
            self.schedule_mirror_tick();
            let _ = self.render(None);
        }
    }

    pub(crate) fn next_live_view_deadline(&self) -> Option<Instant> {
        self.live_views
            .borrow()
            .values()
            .filter_map(|slot| slot.view.next_deadline())
            .min()
    }

    pub(crate) fn tick_live_views(&mut self, now: Instant) -> bool {
        let mut redraw = false;
        let mut state_changed = false;
        {
            let mut live_views = self.live_views.borrow_mut();
            for (_, slot) in live_views.iter_mut() {
                if let Some(update) = slot.view.tick(now) {
                    redraw = true;
                    state_changed |= update.state_changed;
                    slot.redraw(update.redraw);
                }
            }
        }
        if state_changed {
            self.report_live_views();
        }
        redraw
    }

    pub(crate) fn report_live_views(&mut self) {
        if self.live_views.borrow().is_empty() {
            return;
        }
        self.note_live_view_release_keys();
        let reports: Vec<((u32, ClientId), LiveViewReport)> = self
            .live_views
            .borrow_mut()
            .iter_mut()
            .filter_map(|(key, slot)| slot.report_if_changed().map(|report| (*key, report)))
            .collect();
        for ((plugin_pane_id, client_id), report) in reports {
            self.send_live_view_report(plugin_pane_id, client_id, report);
        }
    }

    fn announce_live_view_captures(&mut self) {
        self.live_views.borrow_mut().announce_captures();
    }

    pub(crate) fn sync_live_views(&mut self) {
        if self.live_views.borrow().is_empty() {
            self.announce_live_view_captures();
            return;
        }
        let keys = self.live_views.borrow().keys();
        let mut gone = vec![];
        let mut lost_focus = vec![];
        let mut sizes = vec![];
        for (plugin_pane_id, client_id) in keys {
            if !self.client_may_have_live_views(client_id) {
                gone.push((plugin_pane_id, client_id));
                continue;
            }
            let Some(content) = self.live_view_content_area(plugin_pane_id) else {
                gone.push((plugin_pane_id, client_id));
                continue;
            };
            let captured = self
                .live_views
                .borrow()
                .is_captured(plugin_pane_id, client_id);
            if captured && !self.live_view_takes_input(client_id, plugin_pane_id) {
                lost_focus.push((plugin_pane_id, client_id));
            }
            sizes.push(((plugin_pane_id, client_id), content));
        }
        let mut changed = !gone.is_empty() || !lost_focus.is_empty();
        {
            let mut live_views = self.live_views.borrow_mut();
            for key in &gone {
                live_views.remove(key);
            }
            for key in &lost_focus {
                live_views.release(key, LiveViewRelease::FocusLost);
            }
            for (key, content) in sizes {
                if let Some(slot) = live_views.get_mut(&key) {
                    let size = slot.rect.clamped_to(content.cols, content.rows).size();
                    changed |= slot.view.resized(size);
                }
            }
        }
        self.report_live_views();
        self.announce_live_view_captures();
        if changed {
            self.schedule_mirror_tick();
        }
    }

    pub(crate) fn write_to_live_view(
        &mut self,
        client_id: ClientId,
        key: &Option<KeyWithModifier>,
        raw_bytes: &[u8],
        is_kitty_keyboard_protocol: bool,
    ) -> bool {
        self.release_live_view_if_unreachable(client_id);
        if self.client_is_in_a_prompt(client_id) {
            return false;
        }
        let outcome = self.live_views.borrow_mut().send_key(
            client_id,
            key,
            raw_bytes,
            is_kitty_keyboard_protocol,
        );
        outcome == LiveViewKeyOutcome::Sent
    }

    pub(crate) fn handle_live_view_mouse_event(
        &mut self,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> bool {
        self.release_live_view_if_unreachable(client_id);
        let Some((plugin_pane_id, _)) = self.live_views.borrow().captured_by(client_id) else {
            self.live_views.borrow_mut().cut_drag(client_id);
            return false;
        };
        let key = (plugin_pane_id, client_id);
        let area = self
            .live_view_content_area(plugin_pane_id)
            .and_then(|content| {
                self.live_views
                    .borrow()
                    .get(&key)
                    .map(|slot| slot.area_in(&content))
            });
        let Some(area) = area.filter(|area| !area.is_empty()) else {
            self.live_views.borrow_mut().cut_drag(client_id);
            return false;
        };
        let line = event.position.line();
        let column = event.position.column();
        let is_press = event.event_type == MouseEventType::Press
            && (event.left || event.right || event.middle);
        let is_wheel = event.wheel_up || event.wheel_down;
        let dragging = self
            .live_views
            .borrow()
            .drag_of(client_id)
            .map(|drag| drag.key == key)
            .unwrap_or(false);
        if dragging && !is_press && !is_wheel {
            let clamped_line = (line.max(0) as usize).clamp(area.y, area.y + area.rows - 1);
            let clamped_column = column.clamp(area.x, area.x + area.cols - 1);
            let mut relative = *event;
            relative.position = Position::new(
                (clamped_line - area.y) as i32,
                (clamped_column - area.x) as u16,
            );
            if let Some(slot) = self.live_views.borrow().get(&key) {
                slot.send_mouse(&relative);
            }
            let mut live_views = self.live_views.borrow_mut();
            if event.event_type == MouseEventType::Release {
                live_views.end_drag(client_id);
            } else {
                live_views.drag_moved(client_id, relative.position);
            }
            return true;
        }
        if is_press {
            self.live_views.borrow_mut().cut_drag(client_id);
        }
        let covered = self
            .get_active_tab(client_id)
            .map(|tab| tab.live_view_point_is_covered(client_id, plugin_pane_id, &event.position))
            .unwrap_or(false);
        if covered {
            return false;
        }
        let inside = line >= 0 && area.contains(column, line as usize);
        if inside {
            let mut relative = *event;
            relative.position =
                Position::new((line as usize - area.y) as i32, (column - area.x) as u16);
            if let Some(slot) = self.live_views.borrow().get(&key) {
                slot.send_mouse(&relative);
            }
            if let (true, Some(button)) = (is_press, DragButton::of(event)) {
                self.live_views
                    .borrow_mut()
                    .start_drag(key, button, relative.position);
            }
            return true;
        }
        if is_press {
            self.release_live_view_input_after_the_click(client_id);
        }
        false
    }

    pub(crate) fn toggle_live_view_resize(
        &mut self,
        client_id: ClientId,
        mut completion: Option<NotificationEnd>,
    ) {
        let client_id = if self.get_active_tab(client_id).is_ok() {
            Some(client_id)
        } else {
            self.get_first_client_id()
        };
        let focused = client_id.and_then(|client_id| {
            self.get_active_tab(client_id)
                .ok()
                .and_then(|tab| tab.live_view_focus(client_id))
                .and_then(|pane_id| match pane_id {
                    PaneId::Plugin(plugin_pane_id) => Some((plugin_pane_id, client_id)),
                    PaneId::Terminal(_) => None,
                })
        });
        let result = {
            let mut live_views = self.live_views.borrow_mut();
            match focused.and_then(|key| live_views.get_mut(&key)) {
                None => Err(NO_LIVE_VIEW),
                Some(slot) if slot.view.is_view_only() => Err(LIVE_VIEW_IS_VIEW_ONLY),
                Some(slot) => {
                    let allowed = !slot.view.resize_allowed();
                    slot.view.set_resize_allowed(allowed);
                    Ok(())
                },
            }
        };
        match result {
            Ok(()) => {
                self.report_live_views();
                self.schedule_mirror_tick();
                let _ = self.render(None);
            },
            Err(message) => {
                if let Some(completion) = completion.as_mut() {
                    completion.set_exit_status(1);
                    completion.set_error_message(message.to_owned());
                }
            },
        }
    }
}

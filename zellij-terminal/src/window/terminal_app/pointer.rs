use crate::config::window_state::Shown;
use crate::emulation::damage::Damage;
use crate::emulation::selection::Selection;
use crate::input::clipboard::{self};
use crate::input::links::{self};
use crate::input::momentum::{self, Touch};
use crate::input::mouse::{self};
use crate::render::retained::{self};
use crate::window::App;
use std::time::Instant;
use winit::event::{ElementState, MouseScrollDelta, TouchPhase};
use winit::window::CursorIcon;
use zellij_utils::input::mouse::MouseEvent;
use zellij_utils::ipc::ClientToServerMsg;

impl App {
    pub(in crate::window) fn on_cursor_moved(
        &mut self,
        position: winit::dpi::PhysicalPosition<f64>,
    ) {
        self.show_pointer();
        let (geometry, modifiers) = (self.geometry.get(), self.modifiers);
        let event = self.mouse.pointer.moved(position, geometry, modifiers);
        let target = self.momentum_target();
        self.momentum.pointer_moved(target);
        self.refresh_hover();
        self.refresh_pointer();
        if self.mouse.selecting {
            self.extend_selection();
            return;
        }
        self.point(event);
    }

    pub(in crate::window) fn on_mouse_input(
        &mut self,
        button: winit::event::MouseButton,
        state: ElementState,
    ) {
        if state.is_pressed() {
            self.show_pointer();
            self.stop_momentum();
        }
        if self.selection_gesture(button, state) {
            return;
        }
        if state.is_pressed() {
            self.set_selection(None);
        }
        if self.note_link_click(button, state) {
            return;
        }
        if self.middle_click_pasted(button, state) {
            return;
        }
        let (geometry, modifiers) = (self.geometry.get(), self.modifiers);
        let event = self
            .mouse
            .pointer
            .button(button, state, geometry, modifiers);
        self.point(event);
    }

    pub(in crate::window) fn starts_a_selection(&self) -> bool {
        self.options.shift_drag_selects
            && self.modifiers.shift_key()
            && !(self.modifiers.control_key()
                || self.modifiers.alt_key()
                || self.modifiers.super_key())
    }

    pub(in crate::window) fn selection_gesture(
        &mut self,
        button: winit::event::MouseButton,
        state: ElementState,
    ) -> bool {
        if button != winit::event::MouseButton::Left {
            return false;
        }
        if state.is_pressed() {
            if !self.starts_a_selection() {
                return false;
            }
            let Some(cell) = self.selection_cell() else {
                return false;
            };
            self.note_link_click(button, state);
            self.mouse.selecting = true;
            self.set_selection(Some(Selection::at(cell.0, cell.1)));
            return true;
        }
        if !self.mouse.selecting {
            return false;
        }
        self.mouse.selecting = false;
        if self
            .mouse
            .selection
            .is_some_and(|selection| !selection.is_empty())
        {
            self.mouse.armed_link = None;
        } else {
            self.set_selection(None);
            self.note_link_click(button, state);
        }
        true
    }

    pub(in crate::window) fn selection_cell(&self) -> Option<(usize, usize)> {
        let (x, y) = self.mouse.pointer.cell_if_inside(self.geometry.get())?;
        let size = self.state.size();
        Some((
            (y as usize).min(size.rows.saturating_sub(1)),
            (x as usize).min(size.cols.saturating_sub(1)),
        ))
    }

    pub(in crate::window) fn extend_selection(&mut self) {
        let (Some(selection), Some((row, col))) = (self.mouse.selection, self.selection_cell())
        else {
            return;
        };
        let extended = selection.extended_to(row, col);
        if !extended.is_empty() {
            self.mouse.armed_link = None;
        }
        self.set_selection(Some(extended));
    }

    pub(in crate::window) fn set_selection(&mut self, selection: Option<Selection>) {
        if selection == self.mouse.selection {
            return;
        }
        self.mouse.selection = selection;
        self.retained.set_selection(selection);
        self.schedule_draw();
    }

    pub(in crate::window) fn copy_selection(&mut self) -> bool {
        let Some(selection) = self
            .mouse
            .selection
            .filter(|selection| !selection.is_empty())
        else {
            return false;
        };
        let text = selection.text(self.state.screen());
        self.mouse.selecting = false;
        self.set_selection(None);
        if !text.is_empty() {
            self.with_clipboard(|clipboard| {
                clipboard.set(&text);
                clipboard.set_primary(&text);
            });
        }
        true
    }

    pub(in crate::window) fn middle_click_pasted(
        &mut self,
        button: winit::event::MouseButton,
        state: ElementState,
    ) -> bool {
        if button != winit::event::MouseButton::Middle {
            return false;
        }
        if state.is_pressed() {
            let takes_the_click = mouse::pane_takes_the_click(
                Some(self.mouse.pointer.cell(self.geometry.get())),
                self.state.geometry(),
            );
            let enabled = self.options.middle_click_paste;
            let primary = (enabled && !takes_the_click)
                .then(|| self.with_clipboard(|clipboard| clipboard.get_primary()))
                .flatten()
                .flatten();
            match mouse::middle_click(enabled, primary, takes_the_click) {
                mouse::MiddleClick::Paste(text) => {
                    self.mouse.pointer.swallow_middle();
                    if let Err(e) = self.tell(clipboard::paste_message(text)) {
                        report!("failed to send a primary-selection paste: {}", e);
                    }
                    true
                },
                mouse::MiddleClick::Forward => false,
            }
        } else {
            self.mouse.pointer.take_swallowed_middle()
        }
    }

    pub(in crate::window) fn note_link_click(
        &mut self,
        button: winit::event::MouseButton,
        state: ElementState,
    ) -> bool {
        if button != winit::event::MouseButton::Left {
            return false;
        }
        let cell = self.mouse.pointer.cell_if_inside(self.geometry.get());
        if state.is_pressed() {
            if self.refresh_hover() {
                self.refresh_pointer();
            }
            self.mouse.armed_link = links::armed_by_press(
                self.link_opening(),
                self.pane_wants_mouse(),
                self.modifiers,
                cell,
                self.mouse.hovered_link.as_ref(),
            );
            return self
                .mouse
                .armed_link
                .as_ref()
                .is_some_and(|armed| armed.deliberate);
        }
        let armed = self.mouse.armed_link.take();
        let deliberate = armed.as_ref().is_some_and(|armed| armed.deliberate);
        if let Some(uri) = links::released_on(armed, cell) {
            if let Err(e) = links::open(&uri) {
                report!("{}", e);
            }
        }
        deliberate
    }

    pub(in crate::window) fn link_opening(&self) -> links::Opening {
        links::Opening {
            enabled: self.options.open_links,
            with_shift: self.options.open_links_with_shift,
        }
    }

    pub(in crate::window) fn pane_wants_mouse(&self) -> bool {
        mouse::pane_takes_the_click(
            self.mouse.pointer.cell_if_inside(self.geometry.get()),
            self.state.geometry(),
        )
    }

    pub(in crate::window) fn refresh_hover(&mut self) -> bool {
        let hovered = self
            .mouse
            .pointer
            .cell_if_inside(self.geometry.get())
            .and_then(|(x, y)| links::run_at(self.state.screen(), y as usize, x as usize));
        if hovered == self.mouse.hovered_link {
            return false;
        }
        let mut rows = retained::hover_rows(self.mouse.hovered_link.as_ref());
        rows.extend(retained::hover_rows(hovered.as_ref()));
        self.mouse.hovered_link = hovered;
        self.retained.mark(&Damage::Rows(rows));
        self.schedule_draw();
        true
    }

    pub(in crate::window) fn pointer_shape(&self) -> CursorIcon {
        let cell = self.mouse.pointer.cell_if_inside(self.geometry.get());
        let reachable = links::is_reachable(
            self.link_opening(),
            self.pane_wants_mouse(),
            self.modifiers,
            self.mouse.hovered_link.as_ref(),
        );
        mouse::pointer_shape_over(
            cell,
            self.state.geometry(),
            self.modifiers.control_key(),
            reachable,
        )
    }

    pub(in crate::window) fn refresh_pointer(&mut self) {
        let shape = self.pointer_shape();
        if let Some(surfaces) = &self.surfaces {
            surfaces.window.set_cursor(shape);
        }
    }

    pub(in crate::window) fn hide_pointer(&mut self) {
        if !self.options.hide_pointer_while_typing || self.mouse.pointer_hidden {
            return;
        }
        self.mouse.pointer_hidden = true;
        if let Some(surfaces) = &self.surfaces {
            surfaces.window.set_cursor_visible(false);
        }
    }

    pub(in crate::window) fn show_pointer(&mut self) {
        if !self.mouse.pointer_hidden {
            return;
        }
        self.mouse.pointer_hidden = false;
        if let Some(surfaces) = &self.surfaces {
            surfaces.window.set_cursor_visible(true);
        }
    }

    pub(in crate::window) fn set_focused(&mut self, focused: bool) {
        if focused == self.focused {
            return;
        }
        self.focused = focused;
        self.report_focus();
        self.retained
            .mark(&Damage::Rows(vec![self.state.cursor_position().0]));
        self.schedule_draw();
    }

    pub(in crate::window) fn report_focus(&self) {
        if let Err(e) = self.tell(ClientToServerMsg::HostTerminalFocusChanged {
            focused: self.focused,
        }) {
            report!("failed to report the window focus: {}", e);
        }
    }

    pub(in crate::window) fn toggle_fullscreen(&mut self) {
        let leaving = self.placement.shown == Shown::Fullscreen;
        if leaving {
            self.placement.shown = self.placement.before_fullscreen;
        } else {
            self.placement.before_fullscreen = self.placement.shown;
            self.placement.shown = Shown::Fullscreen;
        }
        let Some(surfaces) = &self.surfaces else {
            return;
        };
        if leaving {
            surfaces.window.set_fullscreen(None);
            if self.placement.shown == Shown::Maximized {
                surfaces.window.set_maximized(true);
            }
        } else {
            surfaces
                .window
                .set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }
    }

    pub(in crate::window) fn on_wheel(&mut self, delta: MouseScrollDelta, phase: TouchPhase) {
        self.on_wheel_at(delta, phase, Instant::now());
    }

    pub(in crate::window) fn on_wheel_at(
        &mut self,
        delta: MouseScrollDelta,
        phase: TouchPhase,
        now: Instant,
    ) {
        self.show_pointer();
        if self.notice.is_some() {
            self.notice_wheel(delta);
            return;
        }
        self.mouse.selecting = false;
        self.set_selection(None);
        let (geometry, modifiers) = (self.geometry.get(), self.modifiers);
        let events = self.mouse.pointer.wheel(delta, geometry, modifiers);
        if self.momentum_enabled() {
            match delta {
                MouseScrollDelta::LineDelta(..) => self.stop_momentum(),
                MouseScrollDelta::PixelDelta(pixels) => {
                    let touch = Touch {
                        phase,
                        pixels: pixels.y,
                        at: now,
                        target: self.momentum_target(),
                        position: self.mouse.pointer.position(geometry),
                        modifiers,
                        cell_height: geometry.cell_height.max(1) as f64,
                        friction: self.options.scroll_momentum_friction,
                    };
                    self.momentum.touchpad(touch);
                },
            }
        }
        self.point(events);
    }

    pub(in crate::window) fn momentum_enabled(&self) -> bool {
        self.wayland && self.options.scroll_momentum
    }

    pub(in crate::window) fn momentum_target(&self) -> Option<momentum::Target> {
        momentum::target_at(
            self.state.geometry(),
            self.mouse.pointer.cell_if_inside(self.geometry.get()),
        )
    }

    pub(in crate::window) fn stop_momentum(&mut self) {
        self.momentum.stop();
    }

    pub(in crate::window) fn tick_momentum(&mut self, now: Instant) -> Vec<MouseEvent> {
        let Some(glide) = self.momentum.tick(now) else {
            return Vec::new();
        };
        let delta =
            MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(0.0, glide.pixels));
        let events = self.mouse.pointer.wheel_at(
            delta,
            glide.position,
            self.geometry.get(),
            glide.modifiers,
        );
        if !events.is_empty() {
            self.momentum.sent();
        }
        events
    }

    pub(in crate::window) fn advance_momentum(&mut self, now: Instant) -> Option<Instant> {
        if !self.momentum.is_coasting() {
            return None;
        }
        let events = self.tick_momentum(now);
        self.point(events);
        self.momentum
            .is_coasting()
            .then(|| now + self.timing.pacer.interval())
    }

    pub(in crate::window) fn on_cursor_left(&mut self) {
        self.show_pointer();
        self.stop_momentum();
        self.mouse.pointer.left();
        self.refresh_pointer();
    }

    pub(in crate::window) fn on_focus_lost(&mut self) {
        self.stop_momentum();
        self.show_pointer();
        if std::mem::take(&mut self.mouse.selecting)
            && self
                .mouse
                .selection
                .is_some_and(|selection| selection.is_empty())
        {
            self.set_selection(None);
        }
        let (geometry, modifiers) = (self.geometry.get(), self.modifiers);
        let events = self.mouse.pointer.focus_lost(geometry, modifiers);
        self.point(events);
    }

    pub(in crate::window) fn point<I: IntoIterator<Item = MouseEvent>>(&mut self, events: I) {
        for event in events {
            if let Err(e) = self.tell(mouse::message(event)) {
                report!("failed to send a mouse event: {}", e);
            }
        }
    }
}

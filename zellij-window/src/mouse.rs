use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::keyboard::ModifiersState;
use winit::window::CursorIcon;
use zellij_utils::input::actions::Action;
use zellij_utils::input::mouse::{MouseEvent, MouseEventType};
use zellij_utils::ipc::ClientToServerMsg;
use zellij_utils::position::Position;
use zellij_utils::structured_render::GeometryRecord;

use crate::connection::Geometry;

const MAX_TICKS_PER_DELTA: usize = 16;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Buttons {
    left: bool,
    right: bool,
    middle: bool,
}

impl Buttons {
    fn any(self) -> bool {
        self.left || self.right || self.middle
    }

    fn get(self, button: Button) -> bool {
        match button {
            Button::Left => self.left,
            Button::Right => self.right,
            Button::Middle => self.middle,
        }
    }

    fn set(&mut self, button: Button, held: bool) {
        match button {
            Button::Left => self.left = held,
            Button::Right => self.right = held,
            Button::Middle => self.middle = held,
        }
    }

    fn only(button: Button) -> Self {
        let mut buttons = Buttons::default();
        buttons.set(button, true);
        buttons
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Button {
    Left,
    Right,
    Middle,
}

impl Button {
    fn of(button: MouseButton) -> Option<Self> {
        match button {
            MouseButton::Left => Some(Button::Left),
            MouseButton::Right => Some(Button::Right),
            MouseButton::Middle => Some(Button::Middle),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MiddleClick {
    Paste(String),
    Forward,
}

pub fn middle_click(
    enabled: bool,
    primary: Option<String>,
    pane_takes_the_click: bool,
) -> MiddleClick {
    if pane_takes_the_click {
        return MiddleClick::Forward;
    }
    match (enabled, primary) {
        (true, Some(text)) if !text.is_empty() => MiddleClick::Paste(text),
        _ => MiddleClick::Forward,
    }
}

pub fn pane_takes_the_click(cell: Option<(u16, u16)>, geometry: &GeometryRecord) -> bool {
    let Some((x, y)) = cell else {
        return false;
    };
    geometry
        .content_pane_at(x, y)
        .map(|pane| pane.selectable() && pane.wants_mouse() && pane.focused())
        .unwrap_or(false)
}

pub fn pointer_shape_over(
    cell: Option<(u16, u16)>,
    geometry: &GeometryRecord,
    openable_link: bool,
) -> CursorIcon {
    if openable_link && cell.is_some() {
        return CursorIcon::Pointer;
    }
    pointer_shape(cell, geometry)
}

pub fn pointer_shape(cell: Option<(u16, u16)>, geometry: &GeometryRecord) -> CursorIcon {
    let Some((x, y)) = cell else {
        return CursorIcon::Default;
    };
    if geometry.panes.is_empty() {
        return CursorIcon::Text;
    }
    if let Some(pane) = geometry.content_pane_at(x, y) {
        if !pane.selectable() || (pane.wants_mouse() && pane.focused()) {
            return CursorIcon::Default;
        }
        return CursorIcon::Text;
    }
    if geometry
        .pane_at(x, y)
        .map(|pane| !pane.selectable())
        .unwrap_or(false)
    {
        return CursorIcon::Default;
    }
    border_shape(x, y, geometry)
}

fn border_shape(x: u16, y: u16, geometry: &GeometryRecord) -> CursorIcon {
    let (x, y) = (x as u32, y as u32);
    let (mut left, mut right, mut above, mut below) = (false, false, false, false);
    for pane in geometry.panes.iter().filter(|pane| pane.selectable()) {
        let content_x = pane.content_x() as u32;
        let content_y = pane.content_y() as u32;
        let content_cols = pane.content_cols() as u32;
        let content_rows = pane.content_rows() as u32;
        if content_cols == 0 || content_rows == 0 {
            continue;
        }
        if y + 1 >= pane.y as u32 && y <= pane.y as u32 + pane.rows as u32 {
            right |= x + 1 == content_x;
            left |= x == content_x + content_cols;
        }
        if x + 1 >= pane.x as u32 && x <= pane.x as u32 + pane.cols as u32 {
            below |= y + 1 == content_y;
            above |= y == content_y + content_rows;
        }
    }
    match (left || right, above || below) {
        (true, true) => {
            if (left && right) || (above && below) {
                CursorIcon::Move
            } else if (left && above) || (right && below) {
                CursorIcon::NwseResize
            } else {
                CursorIcon::NeswResize
            }
        },
        (true, false) => CursorIcon::ColResize,
        (false, true) => CursorIcon::RowResize,
        (false, false) => CursorIcon::Default,
    }
}

#[derive(Debug, Default)]
pub struct PointerState {
    cursor: PhysicalPosition<f64>,
    reported: Option<Position>,
    held: Buttons,
    vertical: f64,
    horizontal: f64,
    swallowed_middle: bool,
    inside: bool,
}

impl PointerState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn moved(
        &mut self,
        cursor: PhysicalPosition<f64>,
        geometry: Geometry,
        modifiers: ModifiersState,
    ) -> Option<MouseEvent> {
        self.cursor = cursor;
        self.inside = true;
        let position = cell_at(cursor, geometry);
        if self.reported == Some(position) {
            return None;
        }
        self.reported = Some(position);
        Some(event(
            MouseEventType::Motion,
            self.held,
            position,
            modifiers,
        ))
    }

    pub fn button(
        &mut self,
        button: MouseButton,
        state: ElementState,
        geometry: Geometry,
        modifiers: ModifiersState,
    ) -> Option<MouseEvent> {
        let button = Button::of(button)?;
        let position = cell_at(self.cursor, geometry);
        self.reported = Some(position);
        match state {
            ElementState::Pressed => {
                self.held.set(button, true);
                Some(event(
                    MouseEventType::Press,
                    Buttons::only(button),
                    position,
                    modifiers,
                ))
            },
            ElementState::Released => {
                if !self.held.get(button) {
                    return None;
                }
                self.held.set(button, false);
                Some(event(
                    MouseEventType::Release,
                    Buttons::only(button),
                    position,
                    modifiers,
                ))
            },
        }
    }

    pub fn wheel(
        &mut self,
        delta: MouseScrollDelta,
        geometry: Geometry,
        modifiers: ModifiersState,
    ) -> Vec<MouseEvent> {
        let (horizontal, vertical) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (x as f64, y as f64),
            MouseScrollDelta::PixelDelta(pixels) => (
                pixels.x / geometry.cell_width.max(1) as f64,
                pixels.y / geometry.cell_height.max(1) as f64,
            ),
        };

        let position = cell_at(self.cursor, geometry);
        let mut events = Vec::new();
        for (ticks, up, down) in [
            (
                accumulate(&mut self.vertical, vertical),
                Wheel::Up,
                Wheel::Down,
            ),
            (
                accumulate(&mut self.horizontal, horizontal),
                Wheel::Right,
                Wheel::Left,
            ),
        ] {
            let wheel = if ticks > 0 { up } else { down };
            for _ in 0..ticks.unsigned_abs() {
                events.push(wheel.event(position, modifiers));
            }
        }
        events
    }

    pub fn swallow_middle(&mut self) {
        self.swallowed_middle = true;
    }

    pub fn take_swallowed_middle(&mut self) -> bool {
        std::mem::take(&mut self.swallowed_middle)
    }

    pub fn focus_lost(&mut self, geometry: Geometry, modifiers: ModifiersState) -> Vec<MouseEvent> {
        self.vertical = 0.0;
        self.horizontal = 0.0;
        self.reported = None;
        self.swallowed_middle = false;
        if !self.held.any() {
            return Vec::new();
        }
        let position = cell_at(self.cursor, geometry);
        let held = std::mem::take(&mut self.held);
        [Button::Left, Button::Right, Button::Middle]
            .into_iter()
            .filter(|button| held.get(*button))
            .map(|button| {
                event(
                    MouseEventType::Release,
                    Buttons::only(button),
                    position,
                    modifiers,
                )
            })
            .collect()
    }

    pub fn entered(&mut self) {
        self.inside = true;
    }

    pub fn left(&mut self) {
        self.reported = None;
        self.inside = false;
    }

    pub fn cell(&self, geometry: Geometry) -> (u16, u16) {
        let position = cell_at(self.cursor, geometry);
        (
            position.column.0.min(u16::MAX as usize) as u16,
            position.line.0.clamp(0, u16::MAX as isize) as u16,
        )
    }

    pub fn cell_if_inside(&self, geometry: Geometry) -> Option<(u16, u16)> {
        self.inside.then(|| self.cell(geometry))
    }
}

#[derive(Debug, Clone, Copy)]
enum Wheel {
    Up,
    Down,
    Left,
    Right,
}

impl Wheel {
    fn event(self, position: Position, modifiers: ModifiersState) -> MouseEvent {
        let mut event = event(
            MouseEventType::Press,
            Buttons::default(),
            position,
            modifiers,
        );
        match self {
            Wheel::Up => event.wheel_up = true,
            Wheel::Down => event.wheel_down = true,
            Wheel::Left => event.wheel_left = true,
            Wheel::Right => event.wheel_right = true,
        }
        event
    }
}

pub fn message(event: MouseEvent) -> ClientToServerMsg {
    ClientToServerMsg::Action {
        action: Action::MouseEvent { event },
        terminal_id: None,
        client_id: None,
        is_cli_client: false,
    }
}

fn accumulate(carried: &mut f64, delta: f64) -> isize {
    if delta == 0.0 {
        return 0;
    }
    if *carried != 0.0 && carried.signum() != delta.signum() {
        *carried = 0.0;
    }
    *carried += delta;
    let whole = carried.trunc();
    *carried -= whole;
    (whole as isize).clamp(
        -(MAX_TICKS_PER_DELTA as isize),
        MAX_TICKS_PER_DELTA as isize,
    )
}

fn cell_at(cursor: PhysicalPosition<f64>, geometry: Geometry) -> Position {
    let column = axis(cursor.x, geometry.cell_width, geometry.cols);
    let line = axis(cursor.y, geometry.cell_height, geometry.rows);
    Position::new(line as i32, column as u16)
}

fn axis(pixels: f64, cell: usize, cells: usize) -> usize {
    if pixels <= 0.0 || cell == 0 {
        return 0;
    }
    let index = (pixels / cell as f64) as usize;
    index.min(cells.saturating_sub(1))
}

fn event(
    event_type: MouseEventType,
    buttons: Buttons,
    position: Position,
    modifiers: ModifiersState,
) -> MouseEvent {
    MouseEvent {
        event_type,
        left: buttons.left,
        right: buttons.right,
        middle: buttons.middle,
        wheel_up: false,
        wheel_down: false,
        wheel_left: false,
        wheel_right: false,
        shift: modifiers.shift_key(),
        alt: modifiers.alt_key(),
        ctrl: modifiers.control_key(),
        position,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::connection::{send, test_attach_at as attach_at, Capabilities};
    use crate::test_server::FakeServer;
    use zellij_utils::structured_render::{
        PaneRect, PANE_FOCUSED, PANE_FRAMED, PANE_SELECTABLE, PANE_WANTS_MOUSE,
    };

    fn geometry() -> Geometry {
        Geometry {
            rows: 40,
            cols: 120,
            cell_width: 10,
            cell_height: 20,
        }
    }

    fn at(x: f64, y: f64) -> PhysicalPosition<f64> {
        PhysicalPosition::new(x, y)
    }

    fn none() -> ModifiersState {
        ModifiersState::empty()
    }

    #[test]
    fn a_middle_click_pastes_the_primary_selection_when_there_is_one() {
        assert_eq!(
            middle_click(true, Some("selected".to_owned()), false),
            MiddleClick::Paste("selected".to_owned())
        );
    }

    #[test]
    fn a_middle_click_is_forwarded_when_nothing_can_be_pasted() {
        assert_eq!(
            middle_click(true, None, false),
            MiddleClick::Forward,
            "a platform with no primary selection must keep the behaviour it had"
        );
        assert_eq!(
            middle_click(true, Some(String::new()), false),
            MiddleClick::Forward,
            "an empty primary selection must not eat the event"
        );
        assert_eq!(
            middle_click(false, Some("selected".to_owned()), false),
            MiddleClick::Forward,
            "turning the option off must hand the button back to the session"
        );
    }

    #[test]
    fn a_middle_click_belongs_to_a_program_that_asked_for_the_mouse() {
        assert_eq!(
            middle_click(true, Some("selected".to_owned()), true),
            MiddleClick::Forward,
            "a program watching the mouse must receive its own middle click"
        );
        assert_eq!(
            middle_click(false, Some("selected".to_owned()), true),
            MiddleClick::Forward
        );
    }

    fn rect(x: u16, y: u16, cols: u16, rows: u16, flags: u8) -> PaneRect {
        PaneRect {
            x,
            y,
            cols,
            rows,
            top: 1,
            bottom: 1,
            left: 1,
            right: 1,
            flags: flags | PANE_FRAMED | PANE_SELECTABLE,
        }
    }

    fn bare(x: u16, y: u16, cols: u16, rows: u16, flags: u8) -> PaneRect {
        PaneRect {
            x,
            y,
            cols,
            rows,
            top: 0,
            bottom: 0,
            left: 0,
            right: 0,
            flags,
        }
    }

    fn two_framed_columns() -> GeometryRecord {
        GeometryRecord {
            panes: vec![
                bare(0, 0, 80, 1, 0),
                rect(0, 1, 40, 19, PANE_FOCUSED),
                rect(40, 1, 40, 19, 0),
                bare(0, 20, 80, 1, 0),
            ],
        }
    }

    #[test]
    fn a_click_over_an_unfocused_pane_watching_the_mouse_still_pastes() {
        let geometry = GeometryRecord {
            panes: vec![
                rect(0, 0, 40, 20, PANE_FOCUSED),
                rect(40, 0, 40, 20, PANE_WANTS_MOUSE),
            ],
        };
        assert!(
            !pane_takes_the_click(Some((45, 5)), &geometry),
            "the session forwards a middle click only to the focused pane"
        );
        assert!(!pane_takes_the_click(Some((5, 5)), &geometry));
        assert!(!pane_takes_the_click(None, &geometry));
    }

    #[test]
    fn a_click_over_the_focused_pane_watching_the_mouse_is_the_programs() {
        let geometry = GeometryRecord {
            panes: vec![rect(0, 0, 40, 20, PANE_FOCUSED | PANE_WANTS_MOUSE)],
        };
        assert!(pane_takes_the_click(Some((5, 5)), &geometry));
        assert!(
            !pane_takes_the_click(Some((0, 0)), &geometry),
            "a click on the frame is not the program's"
        );
    }

    #[test]
    fn the_pointer_is_a_beam_over_the_grid_and_an_arrow_outside_it() {
        let geometry = GeometryRecord::default();
        assert_eq!(pointer_shape(Some((0, 0)), &geometry), CursorIcon::Text);
        assert_eq!(pointer_shape(None, &geometry), CursorIcon::Default);
    }

    #[test]
    fn the_pointer_is_a_beam_over_pane_content_and_an_arrow_over_the_bars() {
        let geometry = two_framed_columns();
        assert_eq!(pointer_shape(Some((5, 5)), &geometry), CursorIcon::Text);
        assert_eq!(pointer_shape(Some((50, 5)), &geometry), CursorIcon::Text);
        assert_eq!(
            pointer_shape(Some((5, 0)), &geometry),
            CursorIcon::Default,
            "the tab bar is not a pane the user types into"
        );
        assert_eq!(
            pointer_shape(Some((5, 20)), &geometry),
            CursorIcon::Default,
            "neither is the status bar"
        );
    }

    #[test]
    fn the_pointer_is_a_hand_over_a_link_it_could_open() {
        let geometry = two_framed_columns();
        assert_eq!(
            pointer_shape_over(Some((5, 5)), &geometry, true),
            CursorIcon::Pointer
        );
        assert_eq!(
            pointer_shape_over(Some((5, 5)), &geometry, false),
            CursorIcon::Text,
            "with no link under it the pointer is the beam it always was"
        );
        assert_eq!(
            pointer_shape_over(None, &geometry, true),
            CursorIcon::Default,
            "a pointer outside the window names no cell and so no link"
        );
    }

    #[test]
    fn a_link_inside_a_pane_watching_the_mouse_still_shows_the_hand() {
        let geometry = GeometryRecord {
            panes: vec![rect(0, 0, 40, 20, PANE_FOCUSED | PANE_WANTS_MOUSE)],
        };
        assert_eq!(
            pointer_shape_over(Some((5, 5)), &geometry, false),
            CursorIcon::Default
        );
        assert_eq!(
            pointer_shape_over(Some((5, 5)), &geometry, true),
            CursorIcon::Pointer,
            "the modified click opens the link whatever the program asked for"
        );
    }

    #[test]
    fn the_pointer_is_an_arrow_while_the_focused_program_watches_the_mouse() {
        let watching = GeometryRecord {
            panes: vec![
                rect(0, 0, 40, 20, PANE_FOCUSED | PANE_WANTS_MOUSE),
                rect(40, 0, 40, 20, PANE_WANTS_MOUSE),
            ],
        };
        assert_eq!(pointer_shape(Some((5, 5)), &watching), CursorIcon::Default);
        assert_eq!(
            pointer_shape(Some((45, 5)), &watching),
            CursorIcon::Text,
            "a background program receives no mouse events, so the beam stays"
        );
    }

    #[test]
    fn the_pointer_is_a_resize_arrow_over_a_framed_pane_border() {
        let geometry = two_framed_columns();
        assert_eq!(
            pointer_shape(Some((39, 5)), &geometry),
            CursorIcon::ColResize,
            "the right border of the left pane is a horizontal drag handle"
        );
        assert_eq!(
            pointer_shape(Some((40, 5)), &geometry),
            CursorIcon::ColResize,
            "so is the left border of the right pane"
        );
        assert_eq!(
            pointer_shape(Some((5, 1)), &geometry),
            CursorIcon::RowResize,
            "the top border of a pane is a vertical drag handle"
        );
        assert_eq!(
            pointer_shape(Some((5, 19)), &geometry),
            CursorIcon::RowResize
        );
    }

    #[test]
    fn the_pointer_is_a_resize_arrow_over_a_frameless_pane_boundary() {
        let geometry = GeometryRecord {
            panes: vec![
                bare(0, 0, 39, 20, PANE_SELECTABLE | PANE_FOCUSED),
                bare(40, 0, 40, 20, PANE_SELECTABLE),
            ],
        };
        assert_eq!(
            pointer_shape(Some((39, 5)), &geometry),
            CursorIcon::ColResize,
            "the gutter between two frameless panes belongs to no pane and is still a handle"
        );
        assert_eq!(pointer_shape(Some((38, 5)), &geometry), CursorIcon::Text);
        assert_eq!(pointer_shape(Some((40, 5)), &geometry), CursorIcon::Text);
    }

    #[test]
    fn the_pointer_is_a_diagonal_arrow_at_the_corner_of_a_framed_pane() {
        let geometry = GeometryRecord {
            panes: vec![
                rect(0, 0, 40, 10, PANE_FOCUSED),
                rect(40, 0, 40, 10, 0),
                rect(0, 10, 40, 10, 0),
                rect(40, 10, 40, 10, 0),
            ],
        };
        assert_eq!(
            pointer_shape(Some((39, 9)), &geometry),
            CursorIcon::NwseResize,
            "the bottom-right corner of the top-left pane"
        );
        assert_eq!(
            pointer_shape(Some((40, 9)), &geometry),
            CursorIcon::NeswResize,
            "the bottom-left corner of the top-right pane"
        );
        assert_eq!(
            pointer_shape(Some((39, 10)), &geometry),
            CursorIcon::NeswResize,
            "the top-right corner of the bottom-left pane"
        );
        assert_eq!(
            pointer_shape(Some((40, 10)), &geometry),
            CursorIcon::NwseResize,
            "the top-left corner of the bottom-right pane"
        );
    }

    #[test]
    fn the_pointer_is_a_move_arrow_where_a_gutter_crosses_another() {
        let geometry = GeometryRecord {
            panes: vec![
                bare(0, 0, 39, 9, PANE_SELECTABLE | PANE_FOCUSED),
                bare(40, 0, 40, 9, PANE_SELECTABLE),
                bare(0, 10, 39, 10, PANE_SELECTABLE),
                bare(40, 10, 40, 10, PANE_SELECTABLE),
            ],
        };
        assert_eq!(pointer_shape(Some((39, 9)), &geometry), CursorIcon::Move);
    }

    #[test]
    fn a_cell_that_belongs_to_nothing_and_borders_nothing_is_an_arrow() {
        let geometry = GeometryRecord {
            panes: vec![rect(0, 0, 10, 5, PANE_FOCUSED)],
        };
        assert_eq!(
            pointer_shape(Some((40, 40)), &geometry),
            CursorIcon::Default
        );
    }

    #[test]
    fn a_swallowed_middle_press_is_remembered_exactly_once() {
        let mut pointer = PointerState::new();
        assert!(!pointer.take_swallowed_middle());
        pointer.swallow_middle();
        assert!(pointer.take_swallowed_middle());
        assert!(
            !pointer.take_swallowed_middle(),
            "one press must swallow one release and no more"
        );
    }

    #[test]
    fn losing_focus_forgets_a_swallowed_middle_press() {
        let mut pointer = PointerState::new();
        pointer.swallow_middle();
        pointer.focus_lost(geometry(), none());
        assert!(!pointer.take_swallowed_middle());
    }

    fn position(line: i32, column: u16) -> Position {
        Position::new(line, column)
    }

    #[test]
    fn pixels_map_onto_cells_by_flooring() {
        assert_eq!(cell_at(at(0.0, 0.0), geometry()), position(0, 0));
        assert_eq!(cell_at(at(9.9, 19.9), geometry()), position(0, 0));
        assert_eq!(cell_at(at(10.0, 20.0), geometry()), position(1, 1));
        assert_eq!(cell_at(at(35.0, 51.0), geometry()), position(2, 3));
    }

    #[test]
    fn a_cursor_outside_the_grid_is_clamped_into_it() {
        assert_eq!(cell_at(at(-4.0, -9.0), geometry()), position(0, 0));
        assert_eq!(cell_at(at(99999.0, 99999.0), geometry()), position(39, 119));
    }

    #[test]
    fn a_hover_is_a_buttonless_motion_event() {
        let mut pointer = PointerState::new();
        let event = pointer.moved(at(35.0, 51.0), geometry(), none()).unwrap();
        assert_eq!(event.event_type, MouseEventType::Motion);
        assert!(!event.left && !event.right && !event.middle);
        assert_eq!(event.position, position(2, 3));
    }

    #[test]
    fn motion_within_one_cell_is_coalesced_away() {
        let mut pointer = PointerState::new();
        assert!(pointer.moved(at(30.0, 40.0), geometry(), none()).is_some());
        assert!(pointer.moved(at(31.0, 41.0), geometry(), none()).is_none());
        assert!(pointer.moved(at(39.9, 59.9), geometry(), none()).is_none());
        assert!(pointer.moved(at(40.0, 40.0), geometry(), none()).is_some());
    }

    #[test]
    fn re_entering_the_window_reports_the_cell_again() {
        let mut pointer = PointerState::new();
        assert!(pointer.moved(at(30.0, 40.0), geometry(), none()).is_some());
        pointer.left();
        assert!(pointer.moved(at(30.0, 40.0), geometry(), none()).is_some());
    }

    #[test]
    fn a_press_reports_the_button_at_the_tracked_cursor() {
        let mut pointer = PointerState::new();
        pointer.moved(at(35.0, 51.0), geometry(), none());
        let event = pointer
            .button(MouseButton::Left, ElementState::Pressed, geometry(), none())
            .unwrap();
        assert_eq!(event.event_type, MouseEventType::Press);
        assert!(event.left);
        assert_eq!(event.position, position(2, 3));
    }

    #[test]
    fn a_drag_carries_the_held_button_and_the_release_names_it() {
        let mut pointer = PointerState::new();
        pointer.button(MouseButton::Left, ElementState::Pressed, geometry(), none());
        let motion = pointer.moved(at(35.0, 51.0), geometry(), none()).unwrap();
        assert_eq!(motion.event_type, MouseEventType::Motion);
        assert!(motion.left);

        let release = pointer
            .button(
                MouseButton::Left,
                ElementState::Released,
                geometry(),
                none(),
            )
            .unwrap();
        assert_eq!(release.event_type, MouseEventType::Release);
        assert!(release.left);
        assert_eq!(release.position, position(2, 3));

        let after = pointer.moved(at(45.0, 51.0), geometry(), none()).unwrap();
        assert!(!after.left);
    }

    #[test]
    fn a_release_without_a_matching_press_reports_nothing() {
        let mut pointer = PointerState::new();
        assert!(pointer
            .button(
                MouseButton::Left,
                ElementState::Released,
                geometry(),
                none()
            )
            .is_none());
    }

    #[test]
    fn the_auxiliary_buttons_are_not_translated() {
        let mut pointer = PointerState::new();
        for button in [
            MouseButton::Back,
            MouseButton::Forward,
            MouseButton::Other(9),
        ] {
            assert!(pointer
                .button(button, ElementState::Pressed, geometry(), none())
                .is_none());
        }
    }

    #[test]
    fn the_right_and_middle_buttons_survive_translation() {
        let mut pointer = PointerState::new();
        let right = pointer
            .button(
                MouseButton::Right,
                ElementState::Pressed,
                geometry(),
                none(),
            )
            .unwrap();
        assert!(right.right && !right.left);
        let middle = pointer
            .button(
                MouseButton::Middle,
                ElementState::Pressed,
                geometry(),
                none(),
            )
            .unwrap();
        assert!(middle.middle && !middle.left);
    }

    #[test]
    fn modifiers_ride_along_with_every_event() {
        let mut pointer = PointerState::new();
        let event = pointer
            .button(
                MouseButton::Left,
                ElementState::Pressed,
                geometry(),
                ModifiersState::CONTROL | ModifiersState::ALT | ModifiersState::SHIFT,
            )
            .unwrap();
        assert!(event.ctrl && event.alt && event.shift);
    }

    #[test]
    fn a_line_of_wheel_travel_is_one_press_typed_tick() {
        let mut pointer = PointerState::new();
        let events = pointer.wheel(MouseScrollDelta::LineDelta(0.0, 1.0), geometry(), none());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, MouseEventType::Press);
        assert!(events[0].wheel_up);
    }

    #[test]
    fn a_downward_wheel_reports_wheel_down() {
        let mut pointer = PointerState::new();
        let events = pointer.wheel(MouseScrollDelta::LineDelta(0.0, -2.0), geometry(), none());
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|event| event.wheel_down));
    }

    #[test]
    fn fractional_wheel_travel_accumulates_until_it_makes_a_tick() {
        let mut pointer = PointerState::new();
        assert!(pointer
            .wheel(MouseScrollDelta::LineDelta(0.0, 0.4), geometry(), none())
            .is_empty());
        assert!(pointer
            .wheel(MouseScrollDelta::LineDelta(0.0, 0.4), geometry(), none())
            .is_empty());
        assert_eq!(
            pointer
                .wheel(MouseScrollDelta::LineDelta(0.0, 0.4), geometry(), none())
                .len(),
            1
        );
    }

    #[test]
    fn reversing_direction_discards_the_carried_remainder() {
        let mut pointer = PointerState::new();
        pointer.wheel(MouseScrollDelta::LineDelta(0.0, 0.9), geometry(), none());
        assert!(pointer
            .wheel(MouseScrollDelta::LineDelta(0.0, -0.9), geometry(), none())
            .is_empty());
    }

    #[test]
    fn pixel_travel_is_measured_in_cell_heights() {
        let mut pointer = PointerState::new();
        let events = pointer.wheel(
            MouseScrollDelta::PixelDelta(at(0.0, -60.0)),
            geometry(),
            none(),
        );
        assert_eq!(events.len(), 3);
        assert!(events.iter().all(|event| event.wheel_down));
    }

    #[test]
    fn horizontal_travel_reports_the_side_wheels() {
        let mut pointer = PointerState::new();
        let right = pointer.wheel(MouseScrollDelta::LineDelta(1.0, 0.0), geometry(), none());
        assert_eq!(right.len(), 1);
        assert!(right[0].wheel_right);
        let left = pointer.wheel(MouseScrollDelta::LineDelta(-1.0, 0.0), geometry(), none());
        assert_eq!(left.len(), 1);
        assert!(left[0].wheel_left);
    }

    #[test]
    fn a_wheel_tick_is_reported_at_the_tracked_cursor() {
        let mut pointer = PointerState::new();
        pointer.moved(at(35.0, 51.0), geometry(), none());
        let events = pointer.wheel(MouseScrollDelta::LineDelta(0.0, 1.0), geometry(), none());
        assert_eq!(events[0].position, position(2, 3));
    }

    #[test]
    fn an_absurd_delta_is_bounded_rather_than_flooding_the_server() {
        let mut pointer = PointerState::new();
        let events = pointer.wheel(MouseScrollDelta::LineDelta(0.0, 1.0e9), geometry(), none());
        assert_eq!(events.len(), MAX_TICKS_PER_DELTA);
    }

    #[test]
    fn losing_focus_releases_every_held_button() {
        let mut pointer = PointerState::new();
        pointer.moved(at(35.0, 51.0), geometry(), none());
        pointer.button(MouseButton::Left, ElementState::Pressed, geometry(), none());
        pointer.button(
            MouseButton::Middle,
            ElementState::Pressed,
            geometry(),
            none(),
        );
        let released = pointer.focus_lost(geometry(), none());
        assert_eq!(released.len(), 2);
        assert!(released
            .iter()
            .all(|event| event.event_type == MouseEventType::Release));
        assert!(released[0].left);
        assert!(released[1].middle);
        assert!(pointer.focus_lost(geometry(), none()).is_empty());
    }

    #[test]
    fn mouse_events_survive_the_protobuf_round_trip_to_the_server() {
        let mut pointer = PointerState::new();
        let mut events = vec![pointer.moved(at(35.0, 51.0), geometry(), none()).unwrap()];
        events.push(
            pointer
                .button(MouseButton::Left, ElementState::Pressed, geometry(), none())
                .unwrap(),
        );
        events.push(pointer.moved(at(95.0, 91.0), geometry(), none()).unwrap());
        events.push(
            pointer
                .button(
                    MouseButton::Left,
                    ElementState::Released,
                    geometry(),
                    none(),
                )
                .unwrap(),
        );
        events.extend(pointer.wheel(
            MouseScrollDelta::LineDelta(0.0, -1.0),
            geometry(),
            ModifiersState::CONTROL,
        ));

        let sent: Vec<ClientToServerMsg> = events.iter().copied().map(message).collect();
        let expected = 8 + sent.len();
        let server = FakeServer::spawn(move |side| side.expect(expected));
        let connection = attach_at(
            &server.path,
            "window-test",
            geometry(),
            Capabilities::default(),
        )
        .unwrap();
        for msg in &sent {
            send(&connection.sender, msg.clone()).unwrap();
        }
        drop(connection);

        assert_eq!(&server.finish()[8..], &sent[..]);
    }
}
